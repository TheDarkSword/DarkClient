//! The startup self-check: does every binding of the running version exist?
//! It goes through reflection because a JNI id lookup initializes the class,
//! and some static initializers have side effects (LWJGL's `VK` loads Vulkan).

use std::collections::{HashMap, HashSet};

use jni::objects::{GlobalRef, JObject, JObjectArray, JString, JValue};
use jni::JNIEnv;

use crate::{Error, Result, Runtime};

const STATIC: i32 = 0x0008;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    Class,
    Method,
    Constructor,
    Field,
}

#[derive(Debug)]
pub struct BindingCheck {
    pub kind: CheckKind,
    pub class: &'static str,
    pub name: &'static str,
    pub descriptor: &'static str,
    pub is_static: bool,
    pub external: bool,
}

impl BindingCheck {
    pub const fn class(class: &'static str, external: bool) -> BindingCheck {
        BindingCheck {
            kind: CheckKind::Class,
            class,
            name: "",
            descriptor: "",
            is_static: false,
            external,
        }
    }

    pub const fn member(
        kind: CheckKind,
        class: &'static str,
        name: &'static str,
        descriptor: &'static str,
        is_static: bool,
        external: bool,
    ) -> BindingCheck {
        BindingCheck {
            kind,
            class,
            name,
            descriptor,
            is_static,
            external,
        }
    }

    fn describe(&self) -> String {
        match self.kind {
            CheckKind::Class => self.class.to_string(),
            CheckKind::Field => format!("{}.{}: {}", self.class, self.name, self.descriptor),
            _ => format!("{}.{}{}", self.class, self.name, self.descriptor),
        }
    }
}

#[derive(Debug, Default)]
pub struct BindingReport {
    pub checked: usize,
    /// External classes this build doesn't ship (LWJGL Vulkan before 26.2).
    pub absent: Vec<&'static str>,
    pub missing: Vec<String>,
}

impl Runtime {
    pub fn verify<'c>(
        &self,
        checks: impl IntoIterator<Item = &'c BindingCheck>,
    ) -> Result<BindingReport> {
        let loader = self.game_class_loader().ok_or(Error::NoClassLoader)?;
        let mut env = self.env()?;
        let mut classes: HashMap<&str, Option<GlobalRef>> = HashMap::new();
        let mut unloadable: HashSet<&str> = HashSet::new();
        let mut report = BindingReport::default();

        for check in checks {
            report.checked += 1;
            let class = classes
                .entry(check.class)
                .or_insert_with(|| load_class(&mut env, &loader, check.class));
            let Some(class) = class.clone() else {
                if unloadable.insert(check.class) {
                    if check.external {
                        report.absent.push(check.class);
                    } else {
                        report.missing.push(check.class.to_string());
                    }
                }
                continue;
            };
            if check.kind == CheckKind::Class {
                continue;
            }
            let found = env.with_local_frame(64, |env| -> Result<bool> {
                find_member(env, &loader, class.as_obj(), check)
            });
            match found {
                Ok(true) => {}
                Ok(false) => report.missing.push(check.describe()),
                Err(error) => {
                    let reason = take_exception(&mut env).unwrap_or_else(|| error.to_string());
                    report
                        .missing
                        .push(format!("{} ({reason})", check.describe()));
                }
            }
        }
        Ok(report)
    }
}

/// `Class.forName(name, false, loader)`: loads without initializing.
fn load_class(env: &mut JNIEnv, loader: &GlobalRef, name: &str) -> Option<GlobalRef> {
    let result = env.with_local_frame(4, |env| -> Result<GlobalRef> {
        let binary = env.new_string(name.replace('/', "."))?;
        let class = env
            .call_static_method(
                "java/lang/Class",
                "forName",
                "(Ljava/lang/String;ZLjava/lang/ClassLoader;)Ljava/lang/Class;",
                &[
                    JValue::Object(&binary),
                    JValue::Bool(0),
                    JValue::Object(loader.as_obj()),
                ],
            )?
            .l()?;
        Ok(env.new_global_ref(class)?)
    });
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.ok()
}

fn find_member<'l>(
    env: &mut JNIEnv<'l>,
    loader: &GlobalRef,
    class: &JObject,
    check: &BindingCheck,
) -> Result<bool> {
    let descriptor = match check.kind {
        CheckKind::Field => format!("(){}", check.descriptor),
        _ => check.descriptor.to_string(),
    };
    let descriptor = env.new_string(descriptor)?;
    let method_type = env
        .call_static_method(
            "java/lang/invoke/MethodType",
            "fromMethodDescriptorString",
            "(Ljava/lang/String;Ljava/lang/ClassLoader;)Ljava/lang/invoke/MethodType;",
            &[JValue::Object(&descriptor), JValue::Object(loader.as_obj())],
        )?
        .l()?;
    let params = env
        .call_method(&method_type, "parameterArray", "()[Ljava/lang/Class;", &[])?
        .l()?;
    let ret = env
        .call_method(&method_type, "returnType", "()Ljava/lang/Class;", &[])?
        .l()?;
    let name = env.new_string(check.name)?;

    if check.kind == CheckKind::Constructor {
        let found = declared(
            env,
            class,
            "getDeclaredConstructor",
            "([Ljava/lang/Class;)Ljava/lang/reflect/Constructor;",
            &[JValue::Object(&params)],
        )?;
        return Ok(found.is_some());
    }

    for owner in hierarchy(env, class)? {
        let member = match check.kind {
            CheckKind::Field => declared(
                env,
                &owner,
                "getDeclaredField",
                "(Ljava/lang/String;)Ljava/lang/reflect/Field;",
                &[JValue::Object(&name)],
            )?,
            _ => declared(
                env,
                &owner,
                "getDeclaredMethod",
                "(Ljava/lang/String;[Ljava/lang/Class;)Ljava/lang/reflect/Method;",
                &[JValue::Object(&name), JValue::Object(&params)],
            )?,
        };
        let Some(member) = member else { continue };
        let ty = match check.kind {
            CheckKind::Field => "getType",
            _ => "getReturnType",
        };
        let actual = env
            .call_method(&member, ty, "()Ljava/lang/Class;", &[])?
            .l()?;
        let modifiers = env.call_method(&member, "getModifiers", "()I", &[])?.i()?;
        // A covariant override can shadow the one we want: keep looking up.
        if env.is_same_object(&actual, &ret)? && (modifiers & STATIC != 0) == check.is_static {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A reflective `getDeclared*` call; `None` when it throws `NoSuch*Exception`.
fn declared<'l>(
    env: &mut JNIEnv<'l>,
    owner: &JObject,
    method: &str,
    descriptor: &str,
    args: &[JValue],
) -> Result<Option<JObject<'l>>> {
    match env.call_method(owner, method, descriptor, args) {
        Ok(value) => Ok(Some(value.l()?)),
        Err(jni::errors::Error::JavaException) => {
            let throwable = env.exception_occurred()?;
            env.exception_clear()?;
            if env.is_instance_of(&throwable, "java/lang/ReflectiveOperationException")? {
                return Ok(None);
            }
            // Something else, like a NoClassDefFoundError: the caller reports it.
            env.throw(throwable)?;
            Err(jni::errors::Error::JavaException.into())
        }
        Err(error) => Err(error.into()),
    }
}

/// Clears the pending exception and returns its `toString()`.
fn take_exception(env: &mut JNIEnv) -> Option<String> {
    if !env.exception_check().ok()? {
        return None;
    }
    env.with_local_frame(4, |env| -> Result<String> {
        let throwable = env.exception_occurred()?;
        env.exception_clear()?;
        let text = env
            .call_method(&throwable, "toString", "()Ljava/lang/String;", &[])?
            .l()?;
        Ok(env.get_string(&JString::from(text))?.into())
    })
    .ok()
}

/// Superclasses first, then interfaces, the order JNI resolves members in.
fn hierarchy<'l>(env: &mut JNIEnv<'l>, class: &JObject) -> Result<Vec<JObject<'l>>> {
    let mut order = vec![env.new_local_ref(class)?];
    loop {
        let parent = env
            .call_method(
                order.last().unwrap(),
                "getSuperclass",
                "()Ljava/lang/Class;",
                &[],
            )?
            .l()?;
        if parent.is_null() {
            break;
        }
        order.push(parent);
    }
    let mut index = 0;
    while index < order.len() {
        let interfaces = env
            .call_method(&order[index], "getInterfaces", "()[Ljava/lang/Class;", &[])?
            .l()?;
        let interfaces = JObjectArray::from(interfaces);
        for i in 0..env.get_array_length(&interfaces)? {
            order.push(env.get_object_array_element(&interfaces, i)?);
        }
        index += 1;
    }
    Ok(order)
}
