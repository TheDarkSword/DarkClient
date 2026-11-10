use std::sync::OnceLock;

use jni::objects::{
    GlobalRef, JClass, JFieldID, JMethodID, JObject, JStaticFieldID, JStaticMethodID, JValue,
    JValueOwned,
};
use jni::signature::{JavaType, Primitive, ReturnType};
use jni::sys::{jobject, jvalue};
use jni::JNIEnv;

use crate::{Error, JavaClass, Object, Result, Runtime};

/// A method or constructor target; its JNI id is looked up once.
#[derive(Debug)]
pub struct Method {
    pub class: &'static str,
    pub name: &'static str,
    pub descriptor: &'static str,
    pub is_static: bool,
    pub ret: ReturnType,
    id: OnceLock<usize>,
}

impl Method {
    pub const fn new(
        class: &'static str,
        name: &'static str,
        descriptor: &'static str,
        is_static: bool,
        ret: ReturnType,
    ) -> Method {
        Method {
            class,
            name,
            descriptor,
            is_static,
            ret,
            id: OnceLock::new(),
        }
    }
}

#[derive(Debug)]
pub struct Field {
    pub class: &'static str,
    pub name: &'static str,
    pub descriptor: &'static str,
    pub is_static: bool,
    id: OnceLock<usize>,
}

impl Field {
    pub const fn new(
        class: &'static str,
        name: &'static str,
        descriptor: &'static str,
        is_static: bool,
    ) -> Field {
        Field {
            class,
            name,
            descriptor,
            is_static,
            id: OnceLock::new(),
        }
    }

    fn return_type(&self) -> ReturnType {
        match primitive(self.descriptor) {
            Some(primitive) => ReturnType::Primitive(primitive),
            None if self.descriptor.starts_with('[') => ReturnType::Array,
            None => ReturnType::Object,
        }
    }

    fn java_type(&self) -> JavaType {
        match primitive(self.descriptor) {
            Some(primitive) => JavaType::Primitive(primitive),
            // JNI only cares that it's an object.
            None => JavaType::Object(String::new()),
        }
    }
}

fn primitive(descriptor: &str) -> Option<Primitive> {
    Some(match descriptor.as_bytes().first()? {
        b'Z' => Primitive::Boolean,
        b'B' => Primitive::Byte,
        b'C' => Primitive::Char,
        b'S' => Primitive::Short,
        b'I' => Primitive::Int,
        b'J' => Primitive::Long,
        b'F' => Primitive::Float,
        b'D' => Primitive::Double,
        b'V' => Primitive::Void,
        _ => return None,
    })
}

/// A thread's JNI env plus the runtime. Every helper frees the local refs it
/// creates, so callers don't need local frames.
pub struct Cx<'l> {
    runtime: &'l Runtime,
    env: JNIEnv<'l>,
    temps: Vec<jobject>,
}

impl Drop for Cx<'_> {
    fn drop(&mut self) {
        for raw in self.temps.drain(..) {
            // SAFETY: each was a live local reference handed to `temp`.
            let _ = self.env.delete_local_ref(unsafe { JObject::from_raw(raw) });
        }
    }
}

fn borrowed(class: &GlobalRef) -> JClass<'static> {
    // SAFETY: the global outlives the call, and a JClass never deletes what it wraps.
    JClass::from(unsafe { JObject::from_raw(class.as_obj().as_raw()) })
}

static ITERABLE_ITERATOR: Method = Method::new(
    "java/lang/Iterable",
    "iterator",
    "()Ljava/util/Iterator;",
    false,
    ReturnType::Object,
);
static ITERATOR_HAS_NEXT: Method = Method::new(
    "java/util/Iterator",
    "hasNext",
    "()Z",
    false,
    ReturnType::Primitive(Primitive::Boolean),
);
static ITERATOR_NEXT: Method = Method::new(
    "java/util/Iterator",
    "next",
    "()Ljava/lang/Object;",
    false,
    ReturnType::Object,
);
static MAP_VALUES: Method = Method::new(
    "java/util/Map",
    "values",
    "()Ljava/util/Collection;",
    false,
    ReturnType::Object,
);

impl Cx<'static> {
    pub fn current() -> Result<Cx<'static>> {
        let runtime = crate::runtime()?;
        Ok(Cx {
            runtime,
            env: runtime.env()?,
            temps: Vec::new(),
        })
    }
}

impl<'l> Cx<'l> {
    pub fn new(runtime: &'l Runtime, env: JNIEnv<'l>) -> Cx<'l> {
        Cx {
            runtime,
            env,
            temps: Vec::new(),
        }
    }

    pub fn runtime(&self) -> &'l Runtime {
        self.runtime
    }

    pub fn env(&mut self) -> &mut JNIEnv<'l> {
        &mut self.env
    }

    /// Returns a new local reference.
    pub fn class(&mut self, name: &str) -> Result<JClass<'l>> {
        self.runtime.resolve_class(&mut self.env, name)
    }

    pub fn is_instance_of(&mut self, object: &JObject, name: &str) -> Result<bool> {
        let class = self.runtime.class_global(&mut self.env, name)?;
        Ok(self.env.is_instance_of(object, borrowed(&class))?)
    }

    pub fn same(&mut self, a: &JObject, b: &JObject) -> Result<bool> {
        Ok(self.env.is_same_object(a, b)?)
    }

    /// Freed when the context is dropped.
    pub fn temp(&mut self, local: JObject<'l>) -> jvalue {
        let raw = local.into_raw();
        self.temps.push(raw);
        jvalue { l: raw }
    }

    pub fn delete(&mut self, local: JObject<'l>) {
        let _ = self.env.delete_local_ref(local);
    }

    pub fn new_string(&mut self, value: &str) -> Result<JObject<'l>> {
        Ok(self.env.new_string(value)?.into())
    }

    /// # Safety
    /// `local` must be null or an instance of `T`'s class.
    pub unsafe fn object<T: JavaClass>(
        &mut self,
        local: JObject<'l>,
        member: &'static str,
    ) -> Result<T> {
        self.object_opt(local)?.ok_or(Error::Null { member })
    }

    /// # Safety
    /// `local` must be null or an instance of `T`'s class.
    pub unsafe fn object_opt<T: JavaClass>(&mut self, local: JObject<'l>) -> Result<Option<T>> {
        if local.is_null() {
            return Ok(None);
        }
        let global = self.env.new_global_ref(&local);
        self.delete(local);
        Ok(Some(T::from_object_unchecked(Object::new(global?))))
    }

    /// Like `object_opt`, but checks the class first.
    pub fn checked<T: JavaClass>(
        &mut self,
        local: JObject<'l>,
        member: &'static str,
    ) -> Result<Option<T>> {
        if local.is_null() {
            return Ok(None);
        }
        let expected = T::class_name()?;
        if !self.is_instance_of(&local, expected)? {
            self.delete(local);
            return Err(Error::WrongClass { member, expected });
        }
        // SAFETY: checked just above.
        unsafe { self.object_opt(local) }
    }

    pub fn string(&mut self, local: JObject<'l>, member: &'static str) -> Result<String> {
        self.string_opt(local)?.ok_or(Error::Null { member })
    }

    pub fn string_opt(&mut self, local: JObject<'l>) -> Result<Option<String>> {
        if local.is_null() {
            return Ok(None);
        }
        let string = jni::objects::JString::from(local);
        let text: Result<String> = self
            .env
            .get_string(&string)
            .map(Into::into)
            .map_err(Error::from);
        self.delete(string.into());
        text.map(Some)
    }

    /// Read once per session, then cached.
    pub fn constant(&mut self, field: &'static Field, member: &'static str) -> Result<GlobalRef> {
        let key = field as *const Field as usize;
        if let Some(cached) = self.runtime.constant(key) {
            return Ok(cached);
        }
        let local = self.get(field, &JObject::null())?.l()?;
        if local.is_null() {
            return Err(Error::Null { member });
        }
        let global = self.env.new_global_ref(&local);
        self.delete(local);
        let global = global?;
        self.runtime.store_constant(key, global.clone());
        Ok(global)
    }

    /// Elements of an `Iterable`, or of a `Map`'s values; `None` from `element` skips one.
    pub fn collect<T>(
        &mut self,
        collection: JObject<'l>,
        map: bool,
        mut element: impl FnMut(&mut Cx<'l>, JObject<'l>) -> Result<Option<T>>,
    ) -> Result<Vec<T>> {
        if collection.is_null() {
            return Ok(Vec::new());
        }
        let iterable = if map {
            let values = self
                .call(&MAP_VALUES, &collection, &[])
                .and_then(|v| Ok(v.l()?));
            self.delete(collection);
            values?
        } else {
            collection
        };
        let iterator = self
            .call(&ITERABLE_ITERATOR, &iterable, &[])
            .and_then(|v| Ok(v.l()?));
        self.delete(iterable);
        let iterator = iterator?;

        let mut out = Vec::new();
        let result = loop {
            match self
                .call(&ITERATOR_HAS_NEXT, &iterator, &[])
                .and_then(|v| Ok(v.z()?))
            {
                Ok(true) => {}
                Ok(false) => break Ok(()),
                Err(error) => break Err(error),
            }
            let next = match self
                .call(&ITERATOR_NEXT, &iterator, &[])
                .and_then(|v| Ok(v.l()?))
            {
                Ok(next) => next,
                Err(error) => break Err(error),
            };
            match element(self, next) {
                Ok(Some(value)) => out.push(value),
                Ok(None) => {}
                Err(error) => break Err(error),
            }
        };
        self.delete(iterator);
        result.map(|()| out)
    }

    fn failed(
        &mut self,
        error: jni::errors::Error,
        class: &'static str,
        name: &'static str,
    ) -> Error {
        if matches!(error, jni::errors::Error::JavaException) {
            let _ = self.env.exception_clear();
            Error::Exception { class, name }
        } else {
            Error::Jni(error)
        }
    }

    fn method_id(&mut self, method: &Method) -> Result<usize> {
        if let Some(id) = method.id.get() {
            return Ok(*id);
        }
        let class = self.runtime.class_global(&mut self.env, method.class)?;
        let class = borrowed(&class);
        let id = if method.is_static {
            self.env
                .get_static_method_id(&class, method.name, method.descriptor)
                .map(|id| id.into_raw() as usize)
        } else {
            self.env
                .get_method_id(&class, method.name, method.descriptor)
                .map(|id| id.into_raw() as usize)
        };
        let id = id.map_err(|_| {
            let _ = self.env.exception_clear();
            Error::MemberNotFound {
                class: method.class,
                name: method.name,
                descriptor: method.descriptor,
            }
        })?;
        let _ = method.id.set(id);
        Ok(id)
    }

    fn field_id(&mut self, field: &Field) -> Result<usize> {
        if let Some(id) = field.id.get() {
            return Ok(*id);
        }
        let class = self.runtime.class_global(&mut self.env, field.class)?;
        let class = borrowed(&class);
        let id = if field.is_static {
            self.env
                .get_static_field_id(&class, field.name, field.descriptor)
                .map(|id| id.into_raw() as usize)
        } else {
            self.env
                .get_field_id(&class, field.name, field.descriptor)
                .map(|id| id.into_raw() as usize)
        };
        let id = id.map_err(|_| {
            let _ = self.env.exception_clear();
            Error::MemberNotFound {
                class: field.class,
                name: field.name,
                descriptor: field.descriptor,
            }
        })?;
        let _ = field.id.set(id);
        Ok(id)
    }

    /// `this` is ignored for statics.
    pub fn call(
        &mut self,
        method: &Method,
        this: &JObject,
        args: &[jvalue],
    ) -> Result<JValueOwned<'l>> {
        let id = self.method_id(method)?;
        let result = if method.is_static {
            let class = self.runtime.class_global(&mut self.env, method.class)?;
            // SAFETY: id, args and return type all come from the same descriptor.
            unsafe {
                self.env.call_static_method_unchecked(
                    borrowed(&class),
                    JStaticMethodID::from_raw(id as _),
                    method.ret.clone(),
                    args,
                )
            }
        } else {
            // SAFETY: as above.
            unsafe {
                self.env.call_method_unchecked(
                    this,
                    JMethodID::from_raw(id as _),
                    method.ret.clone(),
                    args,
                )
            }
        };
        result.map_err(|e| self.failed(e, method.class, method.name))
    }

    pub fn construct(&mut self, method: &Method, args: &[jvalue]) -> Result<JObject<'l>> {
        let id = self.method_id(method)?;
        let class = self.runtime.class_global(&mut self.env, method.class)?;
        // SAFETY: a constructor of `class`, with args matching its descriptor.
        let result = unsafe {
            self.env
                .new_object_unchecked(borrowed(&class), JMethodID::from_raw(id as _), args)
        };
        result.map_err(|e| self.failed(e, method.class, method.name))
    }

    pub fn get(&mut self, field: &Field, this: &JObject) -> Result<JValueOwned<'l>> {
        let id = self.field_id(field)?;
        let result = if field.is_static {
            let class = self.runtime.class_global(&mut self.env, field.class)?;
            // SAFETY: id and type come from the same descriptor.
            unsafe {
                self.env.get_static_field_unchecked(
                    borrowed(&class),
                    JStaticFieldID::from_raw(id as _),
                    field.java_type(),
                )
            }
        } else {
            // SAFETY: as above.
            unsafe {
                self.env
                    .get_field_unchecked(this, JFieldID::from_raw(id as _), field.return_type())
            }
        };
        result.map_err(|e| self.failed(e, field.class, field.name))
    }

    pub fn set(&mut self, field: &Field, this: &JObject, value: JValue) -> Result<()> {
        let id = self.field_id(field)?;
        let result = if field.is_static {
            let class = self.runtime.class_global(&mut self.env, field.class)?;
            // SAFETY: id and type come from the same descriptor.
            unsafe {
                self.env.set_static_field(
                    borrowed(&class),
                    JStaticFieldID::from_raw(id as _),
                    value,
                )
            }
        } else {
            // SAFETY: as above.
            unsafe {
                self.env
                    .set_field_unchecked(this, JFieldID::from_raw(id as _), value)
            }
        };
        result.map_err(|e| self.failed(e, field.class, field.name))
    }
}
