//! Reading and writing of SAILAR modules.

use crate::binary::buffer;
use crate::binary::{LengthSize, RawModule};
use crate::function;
use crate::identifier::{Id, Identifier};
use std::collections::hash_map;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Export {
    Yes,
    No,
}

/// Specifies the version of a SAILAR module file.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct FormatVersion {
    /// The major version number, incremented when backwards incompatible changes are made to the format.
    pub major: u8,
    pub minor: u8,
}

impl FormatVersion {
    /// The minimum version of the format supported by this API.
    pub const MINIMUM_SUPPORTED: &'static Self = &Self { major: 0, minor: 12 };
}

/// Used to help keep track of symbols in modules in order to avoid definitions with duplicate symbols.
#[derive(Clone)]
pub(crate) enum DefinedSymbol {
    Function(Arc<function::Function>),
}

impl DefinedSymbol {
    pub(crate) fn as_id(&self) -> &Id {
        match self {
            Self::Function(function) => function.symbol(),
        }
    }
}

impl std::cmp::PartialEq for DefinedSymbol {
    fn eq(&self, other: &Self) -> bool {
        self.as_id() == other.as_id()
    }
}

impl std::cmp::Eq for DefinedSymbol {}

impl std::hash::Hash for DefinedSymbol {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(self.as_id(), state)
    }
}

impl Debug for DefinedSymbol {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct(match self {
            Self::Function(_) => "Function",
        })
        .field("symbol", &self.as_id())
        .finish_non_exhaustive()
    }
}

#[derive(Debug, PartialEq)]
pub struct DefinedFunction {
    function: Arc<function::Function>,
    definition: function::Definition,
    //index: usize,
    //module: Arc<SomeModuleThing>,
}

impl DefinedFunction {
    pub(crate) fn new(function: Arc<function::Function>, export: Export, body: function::Body) -> Self {
        Self {
            function,
            definition: function::Definition::new(body, export),
        }
    }

    #[inline]
    pub fn function(&self) -> &Arc<function::Function> {
        &self.function
    }

    #[inline]
    pub fn definition(&self) -> &function::Definition {
        &self.definition
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModuleIdentifier {
    name: Identifier,
    version: Box<[usize]>,
}

impl ModuleIdentifier {
    pub fn new(name: Identifier, version: Box<[usize]>) -> Self {
        Self {
            name, version
        }
    }

    #[inline]
    pub fn name(&self) -> &Id {
        self.name.as_id()
    }

    /// Gets the numbers that specify the version of the module, used to disambiguate between modules with the same name.
    #[inline]
    pub fn version(&self) -> &[usize] {
        &self.version
    }
}

/*
pub enum FunctionTemplate {
    Definition,
    Import(Arc<function::Function>, Arc<ModuleIdentifier>),
}

pub struct FunctionInstantiation {

}
*/

pub(crate) type SymbolLookup = rustc_hash::FxHashMap<DefinedSymbol, ()>;

/// A SAILAR module.
pub struct Module {
    contents: Option<RawModule>,
    format_version: FormatVersion,
    length_size: LengthSize,
    identifier: Arc<ModuleIdentifier>,
    symbols: SymbolLookup,
    function_definitions: Vec<DefinedFunction>,
    //function_instantiations: Vec<Arc<>>
    //entry_point: _,
}

mod parser;

pub use parser::{
    Error as ParseError, ErrorKind as ParseErrorKind, InvalidInstructionError as ParsedInstructionError,
    InvalidInstructionKind as ParsedInstructionErrorKind, InvalidMagicError,
};

mod writer;

impl From<Arc<ModuleIdentifier>> for Module {
    fn from(identifier: Arc<ModuleIdentifier>) -> Self {
        let mut length_size = LengthSize::One;
        length_size.resize_to_fit(identifier.name.len());
        length_size.resize_to_fit(identifier.version.len());
        length_size.resize_to_fit_many(identifier.version.iter(), |n| *n);

        Self {
            contents: None,
            format_version: FormatVersion::MINIMUM_SUPPORTED.clone(),
            length_size,
            identifier,
            symbols: SymbolLookup::default(),
            function_definitions: Vec::new(),
        }
    }
}

impl From<ModuleIdentifier> for Module {
    fn from(identifier: ModuleIdentifier) -> Self {
        Self::from(Arc::new(identifier))
    }
}

impl Module {
    pub fn new(name: Identifier, version: Box<[usize]>) -> Self {
        Self::from(ModuleIdentifier::new(name, version))
    }

    #[inline]
    pub fn format_version(&self) -> &FormatVersion {
        &self.format_version
    }

    /// Gets a value indicating the size of length integers in the binary format of the module.
    #[inline]
    pub fn length_size(&self) -> LengthSize {
        self.length_size
    }

    /// Gets the module's identifier, which distinguishes one module from another.
    #[inline]
    pub fn identifier(&self) -> &Arc<ModuleIdentifier> {
        &self.identifier
    }

    /// Writes the bytes binary contents of the module to the specified destination.
    ///
    /// For writers such as [`std::fs::File`], consider wrapping the destination in a [`std::io::BufWriter`].
    pub fn write<W: std::io::Write>(&self, destination: W, buffer_pool: Option<&buffer::Pool>) -> std::io::Result<()> {
        writer::write(self, destination, buffer_pool)
    }

    /// Writes the binary contents of the module to a file, automatically wrapping it in a [`std::io::BufWriter`].
    pub fn write_to_file(&self, destination: std::fs::File, buffer_pool: Option<&buffer::Pool>) -> std::io::Result<()> {
        self.write(std::io::BufWriter::new(destination), buffer_pool)
    }

    /// Returns the binary contents of the module.
    ///
    /// # Examples
    ///
    /// ```
    /// # use sailar::{Identifier, module::Module};
    /// let mut module = Module::new(Identifier::from_str("Testing")?, vec![1, 2, 3].into_boxed_slice());
    /// let contents = module.raw_contents(None).bytes().to_vec();
    /// assert_eq!(sailar::binary::MAGIC.as_slice(), &contents[0..6]);
    /// let format_version = module.format_version();
    /// assert_eq!(&[ format_version.major, format_version.minor ], &contents[6..8]);
    /// assert_eq!(u8::from(sailar::binary::LengthSize::One), contents[8]);
    /// assert_eq!(12, contents[9]);
    /// assert_eq!(7u8, contents[10]); // Module name length
    /// assert_eq!(b"Testing", &contents[11..18]); // Module name
    /// assert_eq!(3u8, contents[18]); // Module version number count
    /// assert_eq!(&[ 1, 2, 3 ], &contents[19..22]); // Module version numbers
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn raw_contents(&mut self, buffer_pool: Option<&buffer::Pool>) -> &RawModule {
        if self.contents.is_none() {
            let mut module_buffer = buffer::RentedOrOwned::with_capacity(512, buffer_pool);

            if let Err(error) = Self::write(self, module_buffer.as_mut_vec(), buffer_pool) {
                unreachable!("unable to write module: {:?}", error)
            }

            self.contents.insert(RawModule::from_vec(module_buffer.into_vec()))
        } else if let Some(existing) = &self.contents {
            existing
        } else {
            unreachable!()
        }
    }

    //pub fn drop_raw_contents
    //pub fn take_raw_contents(&mut self) -> binary::RawModule

    /// Parses a module.
    ///
    /// For sources such as [`std::fs::File`], consider wrapping the reader in a [`std::io::BufReader`].
    #[inline]
    pub fn parse<R: std::io::Read>(source: R, buffer_pool: Option<&buffer::Pool>) -> Result<Self, ParseError> {
        parser::parse(source, buffer_pool)
    }

    /// Parses a module contained a byte slice.
    #[inline]
    pub fn from_slice(bytes: &[u8], buffer_pool: Option<&buffer::Pool>) -> Result<Self, ParseError> {
        Self::parse(bytes, buffer_pool)
    }

    /// Parses a module contained in the byte vector, and stores the bytes alongside the parsed [`Module`].
    ///
    /// The byte vector can be retrieved again by calling [`Module::raw_contents()`].
    pub fn from_vec(bytes: Vec<u8>, buffer_pool: Option<&buffer::Pool>) -> Result<Self, ParseError> {
        let mut module = Self::from_slice(&bytes, buffer_pool)?;
        module.contents = Some(crate::binary::RawModule::from_vec(bytes));
        Ok(module)
    }
}

impl TryFrom<Vec<u8>> for Module {
    type Error = ParseError;

    #[inline]
    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Self::from_vec(bytes, None)
    }
}

#[derive(Clone, Debug)]
pub struct DuplicateSymbolError(DefinedSymbol);

impl DuplicateSymbolError {
    #[inline]
    pub fn symbol(&self) -> &Id {
        self.0.as_id()
    }
}

impl std::fmt::Display for DuplicateSymbolError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "a definition corresponding to the symbol \"{}\" already exists",
            self.symbol()
        )
    }
}

impl std::error::Error for DuplicateSymbolError {}

impl Module {
    /// Adds a function definition or import to this module.
    pub fn add_function(
        &mut self,
        symbol: Identifier,
        signature: Arc<function::Signature>,
        kind: function::Kind,
    ) -> Result<Arc<function::Function>, DuplicateSymbolError> {
        let function = Arc::new(function::Function::new(symbol, signature));

        match kind {
            function::Kind::Defined(definition) => {
                match self.symbols.entry(DefinedSymbol::Function(function.clone())) {
                    hash_map::Entry::Vacant(vacant) => {
                        vacant.insert(());
                    }
                    hash_map::Entry::Occupied(occupied) => return Err(DuplicateSymbolError(occupied.key().clone())),
                }

                if let function::Body::Foreign(ref foreign) = definition.body() {
                    self.length_size.resize_to_fit(foreign.library_name().len());
                    self.length_size.resize_to_fit(foreign.entry_point_name().len());
                }

                self.function_definitions.push(DefinedFunction {
                    function: function.clone(),
                    definition,
                });
            }
        }

        self.length_size.resize_to_fit(function.symbol().len());
        self.length_size.resize_to_fit(function.signature().result_types().len());
        self.length_size.resize_to_fit(function.signature().parameter_types().len());
        // TODO: For each return and argument type, also update the length_size

        self.contents = None;
        Ok(function)
    }

    #[inline]
    pub fn function_definitions(&self) -> &[DefinedFunction] {
        &self.function_definitions
    }
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        struct SymbolLookupDebug<'a>(&'a SymbolLookup);

        impl Debug for SymbolLookupDebug<'_> {
            fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
                f.debug_set().entries(self.0.keys()).finish()
            }
        }

        f.debug_struct("Module")
            .field("format_version", &self.format_version)
            .field("length_size", &self.length_size)
            .field("identifier", &self.identifier)
            .field("symbols", &SymbolLookupDebug(&self.symbols))
            .field("function_definitions", &self.function_definitions)
            .field("contents", &self.contents)
            .finish_non_exhaustive()
    }
}

impl std::cmp::PartialEq for Module {
    /// Checks that the contents of two modules are roughly equivalent.
    fn eq(&self, other: &Self) -> bool {
        let compare_symbols = || {
            if self.symbols.len() != other.symbols.len() {
                return false;
            }

            for definition in self.symbols.keys() {
                match other.symbols.get_key_value(definition) {
                    Some((other_symbol, _)) => match (definition, other_symbol) {
                        (DefinedSymbol::Function(defined_function), DefinedSymbol::Function(other_function)) => {
                            if defined_function != other_function {
                                return false;
                            }
                        }
                    },
                    None => return false,
                }
            }

            true
        };

        self.format_version == other.format_version && self.identifier == other.identifier && compare_symbols()
    }
}
