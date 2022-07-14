//! Module to perform validation of SAILAR code.
//! 
//! Validation ensures that the contents of a SAILAR module are correct, without having to resolve any imports.

use crate::record::{self, Record};
use std::borrow::Cow;

/// A list specifying the kinds of errors that can occur during SAILAR module validation.
/// 
/// Usually used with the [`Error`] type.
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ErrorKind {

}

/// Represents an error that occured during validation of a SAILAR module.
#[derive(Clone, Debug, thiserror::Error)]
#[error(transparent)]
#[repr(transparent)]
pub struct Error(Box<ErrorKind>);

impl Error {
    pub fn from_kind<E: Into<ErrorKind>>(kind: E) -> Self {
        Self(Box::new(kind.into()))
    }
}

impl<E: Into<ErrorKind>> From<E> for Error {
    fn from(error: E) -> Self {
        Self::from_kind(error)
    }
}

pub type ValidationResult<T> = Result<T, Error>;

/// Represents the contents of a SAILAR module.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ModuleContents<'a> {
    identifiers: Vec<Cow<'a, crate::identifier::Id>>,
}

/// Represents a validated SAILAR module.
#[derive(Clone, Debug)]
pub struct ValidModule<'a> {
    contents: ModuleContents<'a>,
}

impl<'a> ValidModule<'a> {
    pub fn from_records_fallible<R, E>(records: R) -> Result<ValidationResult<Self>, E>
    where
        R: IntoIterator<Item = Result<Record<'a>, E>>
    {
        let mut contents = ModuleContents::<'a>::default();

        for data in records.into_iter() {
            match data? {
                Record::Identifier(identifier) => contents.identifiers.push(identifier),
                bad => todo!("validate {:?}", bad),
            }
        }

        // TODO: Perform validation here.

        Ok(Ok(Self { contents }))
    }

    pub fn from_records<R: IntoIterator<Item = Record<'a>>>(records: R) -> ValidationResult<Self> {
        Self::from_records_fallible::<_, std::convert::Infallible>(records.into_iter().map(Ok)).unwrap()
    }

    pub fn contents(&self) -> &ModuleContents<'a> {
        &self.contents
    }

    pub fn into_contents(self) -> ModuleContents<'a> {
        self.contents
    }
}
