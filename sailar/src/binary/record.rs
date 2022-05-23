//! Types that represent records in a SAILAR module binary.

use crate::binary::{index, signature};
use crate::helper::borrow::CowBox;
use crate::{Id, Identifier};
use std::borrow::Cow;

macro_rules! record_types {
    ({
        $($(#[$case_meta:meta])* $case_name:ident$(($($case_argument_name:ident: $case_argument:ty,)*))? = $case_number:literal,)*
    }) => {
        /// Indicates what kind of content is contained in a record.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub enum Type {
            Array = 1,
            $($case_name = $case_number,)*
        }

        impl TryFrom<u8> for Type {
            type Error = InvalidTypeError;
        
            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value {
                    1 => Ok(Self::Array),
                    $(_ if value == $case_number => Ok(Self::$case_name),)*
                    _ => Err(InvalidTypeError { value })
                }
            }
        }

        #[derive(Clone, Debug)]
        #[non_exhaustive]
        pub enum Record<'a> {
            $($(#[$case_meta])* $case_name$(($($case_argument,)*))?,)*
        }

        impl Record<'_> {
            pub fn record_type(&self) -> Type {
                match self {
                    $(Self::$case_name$(($($case_argument_name,)*))? => Type::$case_name,)*
                }
            }
        }
    };
}

record_types!({
    MetadataField(_field: MetadataField<'a>,) = 0,
    Identifier(_identifier: Cow<'a, Id>,) = 2,
    TypeSignature(_signature: Cow<'a, signature::Type>,) = 3,
    FunctionSignature(_signature: Cow<'a, signature::Function>,) = 4,
    Data(_bytes: Cow<'a, DataArray>,) = 5,
    CodeBlock(_code: CowBox<'a, CodeBlock<'a>>,) = 6,
});

impl From<Type> for u8 {
    fn from(value: Type) -> u8 {
        value as u8
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{value:#02X} is not a valid record type")]
pub struct InvalidTypeError {
    value: u8,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum MetadataField<'a> {
    ModuleIdentifier {
        name: Cow<'a, Id>,
        version: CowBox<'a, [usize]>,
    },
}

impl MetadataField<'_> {
    pub fn field_name(&self) -> &'static Id {
        let name = match self {
            Self::ModuleIdentifier { .. } => "id",
        };

        // Safety: all above names are assumed to be valid.
        unsafe { Id::from_str_unchecked(name) }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct DataArray(pub [u8]);

impl DataArray {
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[inline]
    pub fn from_bytes<'a>(bytes: &'a [u8]) -> &'a Self {
        unsafe {
            // Safety: Layout of data array is the same.
            std::mem::transmute::<&'a _, &'a _>(bytes)
        }
    }
}

impl<'a> From<&'a DataArray> for &'a [u8] {
    #[inline]
    fn from(data: &'a DataArray) -> &'a [u8] {
        data.as_bytes()
    }
}

impl<'a> From<&'a [u8]> for &'a DataArray {
    #[inline]
    fn from(bytes: &'a [u8]) -> &'a DataArray {
        DataArray::from_bytes(bytes)
    }
}

impl std::borrow::Borrow<DataArray> for Box<[u8]> {
    #[inline]
    fn borrow(&self) -> &DataArray {
        self.as_ref().into()
    }
}

impl std::borrow::ToOwned for DataArray {
    type Owned = Box<[u8]>;

    #[inline]
    fn to_owned(&self) -> Self::Owned {
        Box::from(self.as_bytes())
    }
}

impl std::cmp::PartialEq<[u8]> for DataArray {
    fn eq(&self, other: &[u8]) -> bool {
        self.as_bytes() == other
    }
}

impl<const N: usize> std::cmp::PartialEq<[u8; N]> for DataArray {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.as_bytes() == other.as_slice()
    }
}

#[derive(Clone, Debug)]
pub struct CodeBlock<'a> {
    register_types: CowBox<'a, [index::TypeSignature]>,
    input_count: usize,
    result_count: usize,
    instructions: Cow<'a, [crate::instruction::Instruction]>,
}

impl<'a> CodeBlock<'a> {
    #[inline]
    fn register_types(&self) -> &[index::TypeSignature] {
        std::borrow::Borrow::borrow(&self.register_types)
    }

    pub fn input_types(&self) -> &[index::TypeSignature] {
        &self.register_types()[0..self.input_count]
    }

    pub fn result_types(&self) -> &[index::TypeSignature] {
        &self.register_types()[self.input_count..self.input_count + self.result_count]
    }

    pub fn temporary_types(&self) -> &[index::TypeSignature] {
        &self.register_types()[self.input_count + self.result_count..]
    }

    pub fn instructions(&self) -> &[crate::instruction::Instruction] {
        &self.instructions
    }
}

impl From<Identifier> for Record<'_> {
    #[inline]
    fn from(identifier: Identifier) -> Self {
        Self::Identifier(Cow::Owned(identifier))
    }
}

impl From<signature::Type> for Record<'_> {
    #[inline]
    fn from(signature: signature::Type) -> Self {
        Self::TypeSignature(Cow::Owned(signature))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn size_of_record_is_acceptable() {
        assert!(std::mem::size_of::<crate::binary::record::Record>() <= 72)
    }
}
