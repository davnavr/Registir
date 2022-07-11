//! Contains types representing errors that can occur during compilation.

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CompilationErrorKind {
    #[error("invalid target triple: {0}")]
    InvalidTargetTriple(String),
    #[error("could not construct target machine for triple {0:?}")]
    InvalidTargetMachine(inkwell::targets::TargetTriple),
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct CompilationError(Box<CompilationErrorKind>);

impl CompilationError {
    pub fn new<E: Into<CompilationErrorKind>>(kind: E) -> Self {
        Self(Box::new(kind.into()))
    }

    pub fn kind(&self) -> &CompilationErrorKind {
        &self.0
    }
}

impl From<CompilationErrorKind> for CompilationError {
    fn from(kind: CompilationErrorKind) -> Self {
        Self::new(kind)
    }
}
