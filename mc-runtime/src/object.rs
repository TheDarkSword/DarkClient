use std::fmt;

use jni::objects::{GlobalRef, JObject};
use jni::signature::{Primitive, ReturnType};
use jni::sys::jvalue;

use crate::{Cx, Error, Method, Result};

/// A Java class with a generated Rust type.
pub trait JavaType {
    /// The class's JNI name on the running version.
    fn class_name() -> Result<&'static str>;
}

/// A Java class whose Rust type is a handle to the live object.
pub trait JavaClass: JavaType + Clone {
    /// # Safety
    /// `object` must be an instance of the class: calling a member on the
    /// wrong class is undefined behaviour in JNI.
    unsafe fn from_object_unchecked(object: Object) -> Self;

    fn as_object(&self) -> &Object;
}

/// A live JVM object (a global ref), usable from any thread.
#[derive(Clone)]
pub struct Object {
    global: GlobalRef,
}

static EQUALS: Method = Method::new(
    "java/lang/Object",
    "equals",
    "(Ljava/lang/Object;)Z",
    false,
    ReturnType::Primitive(Primitive::Boolean),
);
static GET_CLASS: Method = Method::new(
    "java/lang/Object",
    "getClass",
    "()Ljava/lang/Class;",
    false,
    ReturnType::Object,
);
static GET_NAME: Method = Method::new(
    "java/lang/Class",
    "getName",
    "()Ljava/lang/String;",
    false,
    ReturnType::Object,
);

impl Object {
    /// `global` must not be null.
    pub fn new(global: GlobalRef) -> Object {
        Object { global }
    }

    /// `None` for null; the local ref stays the caller's.
    pub fn from_local(local: &JObject) -> Result<Option<Object>> {
        if local.is_null() {
            return Ok(None);
        }
        let mut cx = Cx::current()?;
        Ok(Some(Object::new(cx.env().new_global_ref(local)?)))
    }

    pub fn as_obj(&self) -> &JObject<'static> {
        self.global.as_obj()
    }

    pub fn global(&self) -> &GlobalRef {
        &self.global
    }

    pub fn is_same(&self, other: &Object) -> bool {
        Cx::current()
            .and_then(|mut cx| cx.same(self.as_obj(), other.as_obj()))
            .unwrap_or(false)
    }

    pub fn equals(&self, other: &Object) -> Result<bool> {
        let mut cx = Cx::current()?;
        let args = [jvalue {
            l: other.as_obj().as_raw(),
        }];
        Ok(cx.call(&EQUALS, self.as_obj(), &args)?.z()?)
    }

    pub fn is<T: JavaType>(&self) -> Result<bool> {
        let mut cx = Cx::current()?;
        cx.is_instance_of(self.as_obj(), T::class_name()?)
    }

    pub fn cast<T: JavaClass>(&self) -> Result<Option<T>> {
        Ok(if self.is::<T>()? {
            // SAFETY: checked just above.
            Some(unsafe { T::from_object_unchecked(self.clone()) })
        } else {
            None
        })
    }

    /// Errors unless the object is a `T`.
    pub fn expect<T: JavaType>(&self, member: &'static str) -> Result<()> {
        if self.is::<T>()? {
            Ok(())
        } else {
            Err(Error::WrongClass {
                member,
                expected: T::class_name()?,
            })
        }
    }

    /// Binary name of the runtime class, like `a.b.C$D`.
    pub fn runtime_class_name(&self) -> Result<String> {
        let mut cx = Cx::current()?;
        let class = cx.call(&GET_CLASS, self.as_obj(), &[])?.l()?;
        let name = cx.call(&GET_NAME, &class, &[]).and_then(|v| Ok(v.l()?));
        cx.delete(class);
        cx.string(name?, "Class.getName")
    }
}

impl JavaType for Object {
    fn class_name() -> Result<&'static str> {
        Ok("java/lang/Object")
    }
}

impl JavaClass for Object {
    unsafe fn from_object_unchecked(object: Object) -> Self {
        object
    }

    fn as_object(&self) -> &Object {
        self
    }
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Object({:p})", self.as_obj().as_raw())
    }
}
