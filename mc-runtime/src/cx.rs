use std::sync::OnceLock;

use jni::objects::{
    JClass, JFieldID, JMethodID, JObject, JStaticFieldID, JStaticMethodID, JValue, JValueOwned,
};
use jni::signature::{JavaType, Primitive, ReturnType};
use jni::sys::jvalue;
use jni::JNIEnv;

use crate::{Error, Result, Runtime};

/// A method — or constructor (`<init>`) — of a game class, exactly as the
/// running version declares it. Declared as a `static` by the generated
/// bindings; its JNI id is looked up once and cached.
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

/// A field of a game class, exactly as the running version declares it.
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

    /// The JNI value kind of the field, from its descriptor.
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
            // Only the object/primitive split matters to the JNI call.
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

/// A JNI environment paired with the [`Runtime`] it belongs to — the context
/// every binding call takes.
pub struct Cx<'l> {
    runtime: &'l Runtime,
    env: JNIEnv<'l>,
}

impl<'l> Cx<'l> {
    pub fn new(runtime: &'l Runtime, env: JNIEnv<'l>) -> Cx<'l> {
        Cx { runtime, env }
    }

    pub fn runtime(&self) -> &'l Runtime {
        self.runtime
    }

    /// The underlying JNI environment, for calls the bindings do not cover.
    pub fn env(&mut self) -> &mut JNIEnv<'l> {
        &mut self.env
    }

    /// Resolves a game class by JNI name, through the game class loader.
    pub fn class(&mut self, name: &str) -> Result<JClass<'l>> {
        self.runtime.resolve_class(&mut self.env, name)
    }

    /// Whether `object` is an instance of the class `name`.
    pub fn is_instance_of(&mut self, object: &JObject, name: &str) -> Result<bool> {
        let class = self.class(name)?;
        Ok(self.env.is_instance_of(object, &class)?)
    }

    /// Clears a pending Java exception and reports which member threw it.
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
        let class = self.class(method.class)?;
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
        let class = self.class(field.class)?;
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

    /// Calls `method` on `this` (ignored for a static method) with `args`,
    /// which must match its descriptor.
    pub fn call(
        &mut self,
        method: &Method,
        this: &JObject,
        args: &[jvalue],
    ) -> Result<JValueOwned<'l>> {
        let id = self.method_id(method)?;
        let result = if method.is_static {
            let class = self.class(method.class)?;
            // SAFETY: the id was resolved for this class with this
            // descriptor, which `args` and `ret` follow.
            unsafe {
                self.env.call_static_method_unchecked(
                    &class,
                    JStaticMethodID::from_raw(id as _),
                    method.ret.clone(),
                    args,
                )
            }
        } else {
            // SAFETY: as above, on an instance of the declaring class.
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

    /// Calls the constructor `method` (`<init>`) with `args`.
    pub fn construct(&mut self, method: &Method, args: &[jvalue]) -> Result<JObject<'l>> {
        let id = self.method_id(method)?;
        let class = self.class(method.class)?;
        // SAFETY: the id is a constructor of `class`, and `args` follow its
        // descriptor.
        let result = unsafe {
            self.env
                .new_object_unchecked(&class, JMethodID::from_raw(id as _), args)
        };
        result.map_err(|e| self.failed(e, method.class, method.name))
    }

    /// Reads `field` of `this` (ignored for a static field).
    pub fn get(&mut self, field: &Field, this: &JObject) -> Result<JValueOwned<'l>> {
        let id = self.field_id(field)?;
        let result = if field.is_static {
            let class = self.class(field.class)?;
            // SAFETY: the id was resolved for this class and descriptor.
            unsafe {
                self.env.get_static_field_unchecked(
                    &class,
                    JStaticFieldID::from_raw(id as _),
                    field.java_type(),
                )
            }
        } else {
            // SAFETY: as above, on an instance of the declaring class.
            unsafe {
                self.env
                    .get_field_unchecked(this, JFieldID::from_raw(id as _), field.return_type())
            }
        };
        result.map_err(|e| self.failed(e, field.class, field.name))
    }

    /// Writes `value` to `field` of `this` (ignored for a static field).
    pub fn set(&mut self, field: &Field, this: &JObject, value: JValue) -> Result<()> {
        let id = self.field_id(field)?;
        let result = if field.is_static {
            let class = self.class(field.class)?;
            // SAFETY: the id was resolved for this class and descriptor.
            unsafe {
                self.env
                    .set_static_field(&class, JStaticFieldID::from_raw(id as _), value)
            }
        } else {
            // SAFETY: as above, on an instance of the declaring class.
            unsafe {
                self.env
                    .set_field_unchecked(this, JFieldID::from_raw(id as _), value)
            }
        };
        result.map_err(|e| self.failed(e, field.class, field.name))
    }
}
