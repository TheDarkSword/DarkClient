use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("no JVM found in this process")]
    NoJvm,
    #[error(
        "Minecraft classes not found: obfuscated builds (1.21.11 and older) are not supported"
    )]
    NotMinecraft,
    #[error("class {0} not found at runtime")]
    ClassNotFound(String),
    #[error("{class}.{name}{descriptor} not found at runtime")]
    MemberNotFound {
        class: &'static str,
        name: &'static str,
        descriptor: &'static str,
    },
    #[error("{member} is not available on Minecraft {version}")]
    Unavailable {
        member: &'static str,
        version: String,
    },
    #[error("the Minecraft runtime is not initialised")]
    NoRuntime,
    #[error("the game class loader is not known yet")]
    NoClassLoader,
    #[error("{member} returned null")]
    Null { member: &'static str },
    #[error("{member}: an element is not a {expected}")]
    WrongClass {
        member: &'static str,
        expected: &'static str,
    },
    #[error("{class}: not one of the known enum constants")]
    UnknownConstant { class: &'static str },
    #[error("{class}.{name} threw a Java exception")]
    Exception {
        class: &'static str,
        name: &'static str,
    },
    #[error(transparent)]
    Jni(#[from] jni::errors::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
