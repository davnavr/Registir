//! Module for interacting with SAILAR binary modules.

use crate::binary;
use crate::binary::record;
use crate::identifier::Identifier;

pub type Record = record::Record<'static>;

//pub struct ModuleIdentifierReference

#[derive(Debug)]
pub struct Module {
    loader: std::sync::Weak<crate::loader::State>,
    identifiers: Vec<Identifier>,
}

impl Module {
    pub(crate) fn from_source<S: crate::loader::Source>(source: S) -> Result<Self, S::Error> {
        let mut module = Self {
            loader: Default::default(),
            identifiers: Vec::default(),
        };

        source.iter_records(|record| todo!("record {:?}", record))?;

        Ok(module)
    }

    #[inline]
    pub fn identifiers(&self) -> &[Identifier] {
        &self.identifiers
    }
}
