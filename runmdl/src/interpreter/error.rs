use super::{call_stack, JumpTarget, RegisterIndex};

pub type LoaderError = getmdl::loader::Error;

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    #[error(transparent)]
    LoadError(#[from] LoaderError),
    #[error("call stack underflow occured")]
    CallStackUnderflow,
    #[error("exceeded maximum call stack depth ({0})")]
    CallStackOverflow(call_stack::StackCapacity),
    #[error("end of block unexpectedly reached")]
    UnexpectedEndOfBlock,
    #[error("undefined register {0}")]
    UndefinedRegister(RegisterIndex),
    #[error("undefined block {0}")]
    UndefinedBlock(JumpTarget),
    #[error("expected {expected} input values but got {actual}")]
    InputCountMismatch { expected: usize, actual: usize },
    #[error("expected {expected} result values but got {actual}")]
    ResultCountMismatch { expected: usize, actual: usize },
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    stack_trace: call_stack::Trace,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, stack_trace: call_stack::Trace) -> Self {
        Self { kind, stack_trace }
    }

    pub(crate) fn with_no_stack_trace(kind: ErrorKind) -> Self {
        Self::new(kind, Vec::new())
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn stack_trace(&self) -> &[call_stack::TraceFrame] {
        &self.stack_trace
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

impl std::error::Error for Error {}
