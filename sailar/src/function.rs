//! Manipulation of SAILAR function definitions and function imports.

use crate::block;
use crate::module::Export;
use crate::type_system::Any;
use crate::{Id, Identifier};
use std::sync::Arc;

/// Represents a SAILAR function signature.
#[derive(Clone, Default, Debug, Eq, Hash, PartialEq)]
pub struct Signature {
    result_types: Box<[Any]>,
    parameter_types: Box<[Any]>,
}

impl Signature {
    pub fn new<R: Into<Box<[Any]>>, I: Into<Box<[Any]>>>(result_types: R, parameter_types: I) -> Self {
        Self {
            result_types: result_types.into(),
            parameter_types: parameter_types.into(),
        }
    }

    #[inline]
    pub fn result_types(&self) -> &[Any] {
        &self.result_types
    }

    #[inline]
    pub fn parameter_types(&self) -> &[Any] {
        &self.parameter_types
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct Function {
    symbol: Identifier,
    signature: Arc<Signature>,
}

impl Function {
    pub(crate) fn new(symbol: Identifier, signature: Arc<Signature>) -> Arc<Self> {
        Arc::new(Self { symbol, signature })
    }

    #[inline]
    pub fn symbol(&self) -> &Id {
        self.symbol.as_id()
    }

    #[inline]
    pub fn signature(&self) -> &Signature {
        &self.signature
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForeignBody {
    library_name: Arc<Identifier>,
    entry_point_name: Identifier,
}

impl ForeignBody {
    pub fn new(library_name: Arc<Identifier>, entry_point_name: Identifier) -> Self {
        Self {
            library_name,
            entry_point_name,
        }
    }

    /// The name of the library that the function is defined in.
    #[inline]
    pub fn library_name(&self) -> &Arc<Identifier> {
        &self.library_name
    }

    #[inline]
    pub fn entry_point_name(&self) -> &Id {
        self.entry_point_name.as_id()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Body {
    /// Indicates that a function's body is defined in its module, providing the entry block that is executed.
    Defined(Arc<block::Block>),
    /// Indicates that a function's body is defined elsewhere, used by the foreign function interface or to call functions
    /// defined in the runtime.
    Foreign(Box<ForeignBody>),
}

bitflags::bitflags! {
    pub struct Flags: u8 {
        const NONE = 0;
        const EXPORT = 0b0000_0001;
        const FOREIGN = 0b000_0010;
    }
}

impl Body {
    pub fn flags(&self) -> Flags {
        match self {
            Self::Defined(_) => Flags::NONE,
            Self::Foreign(_) => Flags::FOREIGN,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Definition {
    body: Body,
    export: Export,
}

impl Definition {
    pub fn new(body: Body, export: Export) -> Self {
        Self { body, export }
    }

    #[inline]
    pub fn body(&self) -> &Body {
        &self.body
    }

    #[inline]
    pub fn is_export(&self) -> Export {
        self.export
    }

    pub fn flags(&self) -> Flags {
        let mut flags = self.body().flags();
        if self.export == Export::Yes {
            flags |= Flags::EXPORT;
        }
        flags
    }
}
