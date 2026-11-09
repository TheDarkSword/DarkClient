use thiserror::Error;

/// Anything that can go wrong talking to the game through JNI.
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
    #[error("{class}.{name} threw a Java exception")]
    Exception {
        class: &'static str,
        name: &'static str,
    },
    #[error(transparent)]
    Jni(#[from] jni::errors::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
