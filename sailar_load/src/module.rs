//! Module for interacting with SAILAR binary modules.

use crate::function;
use crate::symbol::{DuplicateSymbolError, Symbol};
use sailar::identifier::Id;
use sailar::record;
use std::borrow::Cow;
use std::collections::hash_map;
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Weak};

pub type Record = record::Record<'static>;

pub type ModuleIdentifier = record::ModuleIdentifier<'static>;

pub struct SymbolLookup {
    lookup: rustc_hash::FxHashMap<Symbol, ()>,
}

impl SymbolLookup {
    pub fn get<S: ?Sized>(&self, symbol: &S) -> Option<&Symbol>
    where
        Symbol: std::borrow::Borrow<S>,
        S: std::hash::Hash + std::cmp::Eq,
    {
        self.lookup.get_key_value(symbol).map(|(k, _)| k)
    }

    fn try_insert<S: Into<Symbol>>(&mut self, symbol: Option<S>) -> Result<(), DuplicateSymbolError> {
        if let Some(s) = symbol {
            match self.lookup.entry(s.into()) {
                hash_map::Entry::Occupied(occupied) => Err(DuplicateSymbolError::new(occupied.key().clone())),
                hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(());
                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }
}

pub struct Module {
    loader: Weak<crate::State>,
    identifiers: Vec<Cow<'static, Id>>,
    module_identifier: Option<Arc<ModuleIdentifier>>,
    symbols: SymbolLookup,
    function_definitions: Vec<Arc<function::Definition>>,
    //function_instantiations: Vec<Arc<function::Instantiation>>,
}

impl Module {
    pub(crate) fn from_source<S: crate::Source>(source: S, loader: Weak<crate::State>) -> Result<Arc<Self>, S::Error> {
        let mut error = None;
        let module = Arc::new_cyclic(|this| {
            let mut module = Self {
                loader,
                identifiers: Vec::default(),
                module_identifier: None,
                symbols: SymbolLookup {
                    lookup: Default::default(),
                },
                function_definitions: Vec::default(),
                //function_instantiations: Vec::default(),
            };

            error = source
                .iter_records(|record| match record {
                    Record::MetadataField(field) => match field {
                        record::MetadataField::ModuleIdentifier(identifier) => {
                            module.module_identifier = Some(Arc::new(identifier))
                        }
                        bad => todo!("unknown metadata field {:?}", bad),
                    },
                    Record::Identifier(identifier) => module.identifiers.push(identifier),
                    Record::FunctionDefinition(definition) => {
                        let function = function::Definition::new(definition, this.clone());
                        module.symbols.try_insert(function.to_symbol()).expect("TODO: handle duplicate symbol error");
                        module.function_definitions.push(function);
                    }
                    // Record::FunctionInstantiation(instantiation) => module
                    //     .function_instantiations
                    //     .push(function::Instantiation::new(instantiation, module_weak.clone())),
                    bad => todo!("unsupported {:?}", bad),
                })
                .err();

            module
        });

        if let Some(e) = error {
            Err(e)
        } else {
            Ok(module)
        }
    }

    /// Indicates if the module has an identifier (a name and version).
    pub fn is_anonymous(&self) -> bool {
        self.module_identifier.is_none()
    }

    pub fn loader(&self) -> &Weak<crate::State> {
        &self.loader
    }

    pub fn symbols(&self) -> &SymbolLookup {
        &self.symbols
    }

    pub fn identifiers(&self) -> &[Cow<'static, Id>] {
        &self.identifiers
    }

    /// Gets an optional weak reference to the module's identifier, indicating its name and version.
    pub fn module_identifier(&self) -> Option<&Arc<ModuleIdentifier>> {
        self.module_identifier.as_ref()
    }

    // pub fn function_definitions(&self) -> &[Arc<function::Definition>] {
    //     &self.function_definitions
    // }

    // pub fn function_instantiations(&self) -> &[Arc<function::Instantiation>] {
    //     &self.function_instantiations
    // }
}

impl Debug for Module {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("identifiers", &self.identifiers)
            .field("module_identifier", &self.module_identifier)
            .finish()
    }
}

impl std::cmp::PartialEq for Module {
    fn eq(&self, other: &Self) -> bool {
        self.module_identifier == other.module_identifier
    }
}

impl std::cmp::Eq for Module {}

pub(crate) fn module_weak_eq(a: &Weak<Module>, b: &Weak<Module>) -> bool {
    a.ptr_eq(b) || a.upgrade().zip(b.upgrade()).map_or(false, |(a, b)| a == b)
}
