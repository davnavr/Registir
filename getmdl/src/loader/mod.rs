use registir::format;
use std::cell::RefCell;
use std::collections::{hash_map, HashMap};
use typed_arena::Arena as TypedArena;

mod names;

pub use format::{Identifier, ModuleIdentifier};
pub use names::{FullIdentifier, FullMethodIdentifier, FullTypeIdentifier};

pub struct Module<'a> {
    source: format::Module,
    type_arena: TypedArena<Type<'a>>,
    loaded_types: RefCell<HashMap<usize, &'a Type<'a>>>,
    //loaded_fields: ,
    method_arena: TypedArena<Method<'a>>,
    loaded_methods: RefCell<HashMap<usize, &'a Method<'a>>>,
    type_lookup_cache: RefCell<HashMap<Identifier, &'a Type<'a>>>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum LoadError {
    IndexOutOfBounds(format::numeric::UInteger),
    Other(Box<dyn std::error::Error>),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexOutOfBounds(index) => write!(
                f,
                "attempted load with index {}, which is out of bounds",
                index
            ),
            Self::Other(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for LoadError {}

pub type LoadResult<T> = Result<T, LoadError>;

fn read_index<
    I: TryInto<usize> + Copy + Into<format::numeric::UInteger>,
    T,
    R: FnOnce(usize) -> LoadResult<T>,
>(
    index: I,
    reader: R,
) -> LoadResult<T> {
    index
        .try_into()
        .map_err(|_| LoadError::IndexOutOfBounds(index.into()))
        .and_then(reader)
}

fn read_index_from<
    'a,
    I: TryInto<usize> + Copy + Into<format::numeric::UInteger>,
    T,
    U,
    R: FnOnce(&'a T) -> LoadResult<U>,
>(
    index: I,
    s: &'a [T],
    reader: R,
) -> LoadResult<U> {
    read_index::<I, U, _>(index, |actual_index| {
        s.get(actual_index)
            .ok_or(LoadError::IndexOutOfBounds(index.into()))
            .and_then(reader)
    })
}

impl<'a> Module<'a> {
    fn new(source: format::Module) -> Self {
        Self {
            source,
            type_arena: TypedArena::new(),
            loaded_types: RefCell::default(),
            method_arena: TypedArena::new(),
            loaded_methods: RefCell::default(),
            type_lookup_cache: RefCell::default(),
        }
    }

    pub fn identifier(&'a self) -> &'a ModuleIdentifier {
        &self.source.header.0.identifier
    }

    fn load_raw<
        T: 'a,
        I: TryInto<usize> + Copy + Into<format::numeric::UInteger>,
        L,
        F: FnOnce(usize) -> Option<&'a T>,
        C: FnOnce(&'a T) -> LoadResult<L>,
    >(
        lookup: &'a RefCell<HashMap<usize, &'a L>>,
        arena: &'a TypedArena<L>,
        loader: F,
        constructor: C,
        index: I,
    ) -> LoadResult<&'a L> {
        read_index::<_, &'a L, _>(index, |raw_index| {
            match lookup.borrow_mut().entry(raw_index) {
                hash_map::Entry::Occupied(occupied) => Ok(occupied.get()),
                hash_map::Entry::Vacant(vacant) => match loader(raw_index) {
                    Some(source) => {
                        let loaded = arena.alloc(constructor(source)?);
                        vacant.insert(loaded);
                        Ok(loaded)
                    }
                    None => Err(LoadError::IndexOutOfBounds(index.into())),
                },
            }
        })
    }

    pub fn load_identifier_raw(
        &'a self,
        index: format::indices::Identifier,
    ) -> LoadResult<&'a Identifier> {
        read_index_from(index, &self.source.identifiers, Ok)
    }

    pub fn load_type_raw(
        &'a self,
        index: format::indices::TypeDefinition,
    ) -> LoadResult<&'a Type<'a>> {
        Self::load_raw(
            &self.loaded_types,
            &self.type_arena,
            |index| self.source.definitions.0.defined_types.0.get(index),
            |source| Ok(Type::new(self, source)),
            index,
        )
    }

    pub fn load_method_raw(
        &'a self,
        index: format::indices::MethodDefinition,
    ) -> LoadResult<&'a Method<'a>> {
        Self::load_raw(
            &self.loaded_methods,
            &self.method_arena,
            |index| self.source.definitions.0.defined_methods.0.get(index),
            |source| Ok(Method::new(self.load_type_raw(source.owner)?, source)),
            index,
        )
    }

    pub fn entry_point(&'a self) -> LoadResult<Option<&'a Method<'a>>> {
        match self.source.entry_point.0 {
            Some(main_index) => self.load_method_raw(main_index).map(Some),
            None => Ok(None),
        }
    }

    pub fn load_type_signature_raw(
        &'a self,
        index: format::indices::TypeSignature,
    ) -> LoadResult<&'a format::TypeSignature> {
        read_index_from(index, &self.source.type_signatures.0, Ok)
    }

    fn collect_type_signatures_raw(
        &'a self,
        indices: &'a [format::indices::TypeSignature],
    ) -> LoadResult<Vec<&'a format::TypeSignature>> {
        let mut types = Vec::with_capacity(indices.len());
        for index in indices {
            types.push(self.load_type_signature_raw(*index)?);
        }
        Ok(types)
    }

    pub fn load_code_raw(&'a self, index: format::indices::Code) -> LoadResult<&'a format::Code> {
        read_index_from(index, &self.source.method_bodies.0, Ok)
    }

    pub fn lookup_type(&'a self, name: &Identifier) -> Result<&'a Type<'a>, ()> {
        match self.type_lookup_cache.borrow_mut().entry(name.clone()) {
            hash_map::Entry::Occupied(occupied) => Ok(occupied.get()),
            hash_map::Entry::Vacant(vacant) => {
                let result = self
                    .source
                    .definitions
                    .0
                    .defined_types
                    .0
                    .iter()
                    .enumerate()
                    .find(|(_, definition)| {
                        self.load_identifier_raw(definition.name)
                            .ok()
                            .filter(|&type_name| type_name == name)
                            .is_some()
                    })
                    .map(|(index, _)| {
                        self.load_type_raw(
                            format::indices::TypeDefinition::try_from(index).unwrap(),
                        )
                        .unwrap()
                    });
                if let Some(definition) = result {
                    vacant.insert(definition);
                }
                result.ok_or(())
            }
        }
    }
}

pub struct Method<'a> {
    source: &'a format::Method,
    owner: &'a Type<'a>,
}

pub struct MethodSignatureTypes<'a> {
    pub return_types: Vec<&'a format::TypeSignature>,
    pub parameter_types: Vec<&'a format::TypeSignature>,
}

impl<'a> Method<'a> {
    fn new(owner: &'a Type<'a>, source: &'a format::Method) -> Self {
        Self { source, owner }
    }

    pub fn declaring_module(&'a self) -> &'a Module<'a> {
        self.owner.declaring_module()
    }

    pub fn name(&'a self) -> LoadResult<&'a Identifier> {
        self.declaring_module()
            .load_identifier_raw(self.source.name)
    }

    pub fn raw_body(&'a self) -> &'a format::MethodBody {
        &self.source.body
    }

    /// Gets the raw method blocks that make up the method's body, if it is defined.
    pub fn raw_code(&'a self) -> LoadResult<Option<&'a format::Code>> {
        use format::MethodBody;

        match self.raw_body() {
            MethodBody::Defined(index) => self.declaring_module().load_code_raw(*index).map(Some),
            MethodBody::Abstract | MethodBody::External { .. } => Ok(None),
        }
    }

    pub fn raw_signature(&'a self) -> LoadResult<&'a format::MethodSignature> {
        read_index_from(
            self.source.signature,
            &self.declaring_module().source.method_signatures.0,
            Ok,
        )
    }

    pub fn raw_signature_types(&'a self) -> LoadResult<MethodSignatureTypes<'a>> {
        let signature = self.raw_signature()?;
        Ok(MethodSignatureTypes {
            return_types: self
                .declaring_module()
                .collect_type_signatures_raw(&signature.return_types)?,
            parameter_types: self
                .declaring_module()
                .collect_type_signatures_raw(&signature.parameter_types)?,
        })
    }

    pub fn identifier(&'a self) -> LoadResult<FullMethodIdentifier> {
        Ok(FullMethodIdentifier::new(
            self.owner.identifier()?,
            self.name()?.clone(),
        ))
    }
}

impl<'a> std::cmp::PartialEq for Method<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.source as *const _ == other.source as *const _
            && self.owner as *const _ == other.owner as *const _
    }
}

impl<'a> std::cmp::Eq for Method<'a> {}

impl<'a> std::hash::Hash for Method<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.source as *const format::Method).hash(state);
        (self.owner as *const Type<'a>).hash(state)
    }
}

pub struct Type<'a> {
    source: &'a format::Type,
    module: &'a Module<'a>,
}

impl<'a> Type<'a> {
    fn new(module: &'a Module<'a>, source: &'a format::Type) -> Self {
        Self { source, module }
    }

    pub fn name(&'a self) -> LoadResult<&'a Identifier> {
        self.declaring_module()
            .load_identifier_raw(self.source.name)
    }

    pub fn declaring_module(&'a self) -> &'a Module<'a> {
        self.module
    }

    pub fn try_lookup_method(&'a self, name: &Identifier) -> LoadResult<Vec<&'a Method<'a>>> {
        let mut matches = Vec::new();
        for &index in &self.source.methods.0 {
            // TODO: Since names are simply being checked, could avoid loading of methods.
            let method = self.module.load_method_raw(index)?;
            if method.name()? == name {
                matches.push(method)
            }
        }
        Ok(matches)
    }

    pub fn lookup_method(&'a self, name: &Identifier) -> Vec<&'a Method<'a>> {
        self.try_lookup_method(name).unwrap_or(Vec::new())
    }

    pub fn identifier(&'a self) -> LoadResult<FullTypeIdentifier> {
        Ok(FullTypeIdentifier::new(
            self.module.identifier().clone(),
            self.name()?.clone(),
        ))
    }
}

pub struct Loader<'a> {
    module_arena: TypedArena<Module<'a>>,
    loaded_modules: RefCell<HashMap<ModuleIdentifier, &'a Module<'a>>>,
}

impl<'a> Loader<'a> {
    fn new_empty() -> Self {
        Self {
            module_arena: TypedArena::new(),
            loaded_modules: RefCell::new(HashMap::new()),
        }
    }

    fn load_module_raw(&'a self, source: format::Module) -> &'a Module<'a> {
        let identifier = source.header.0.identifier.clone();
        match self.loaded_modules.borrow_mut().entry(identifier) {
            hash_map::Entry::Vacant(vacant) => {
                let loaded = self.module_arena.alloc(Module::new(source));
                vacant.insert(loaded);
                loaded
            }
            hash_map::Entry::Occupied(occupied) => occupied.get(),
        }
    }

    pub fn initialize(
        loader: &'a mut Option<Loader<'a>>,
        application: format::Module,
    ) -> (&'a Self, &'a Module<'a>) {
        let loaded = loader.insert(Loader::new_empty());
        (loaded, loaded.load_module_raw(application))
    }

    // TODO: How to force loading of a module if it is an import of one of the already loaded modules?
    pub fn lookup_module(&'a self, name: &ModuleIdentifier) -> Result<&'a Module<'a>, ()> {
        self.loaded_modules.borrow().get(name).copied().ok_or(())
    }

    pub fn lookup_type(&'a self, name: &FullTypeIdentifier) -> Result<&'a Type<'a>, ()> {
        self.lookup_module(name.module_name())?
            .lookup_type(&name.type_name())
    }

    pub fn lookup_method(&'a self, name: &FullMethodIdentifier) -> Vec<&'a Method<'a>> {
        self.lookup_type(name.type_name())
            .map(|type_definition| type_definition.lookup_method(name.method_name()))
            .unwrap_or(Vec::new())
    }
}
