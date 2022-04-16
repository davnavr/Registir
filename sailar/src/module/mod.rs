//! Reading and writing of SAILAR modules.

use crate::binary::buffer;
use crate::binary::{LengthSize, RawModule};
use crate::function;
use crate::identifier::{Id, Identifier};
use std::sync::Arc;

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
    pub const MINIMUM_SUPPORTED: &'static Self = &Self {
        major: 0,
        minor: 12,
    };
}

/// A SAILAR module.
#[derive(Debug)]
pub struct Module {
    contents: Option<RawModule>,
    format_version: FormatVersion,
    length_size: LengthSize,
    name: Identifier,
    version: Box<[usize]>,
}

mod parser;

pub use parser::{Error as ParseError, ErrorKind as ParseErrorKind, InvalidMagicError};

mod writer;

impl Module {
    pub fn new(name: Identifier, version: Box<[usize]>) -> Self {
        let mut length_size = LengthSize::One;
        length_size.resize_to_fit(name.len());
        length_size.resize_to_fit_many(version.iter(), |n| *n);

        Self {
            contents: None,
            format_version: FormatVersion::MINIMUM_SUPPORTED.clone(),
            length_size,
            name,
            version,
        }
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

    /// Gets the name of the module.
    #[inline]
    pub fn name(&self) -> &Id {
        self.name.as_id()
    }

    /// Gets the numbers that specify the version of the module, used to disambiguate between modules with the same name.
    #[inline]
    pub fn version(&self) -> &[usize] {
        &self.version
    }

    /// Writes the bytes binary contents of the module to the specified destination.
    ///
    /// For writers such as [`std::fs::File`], consider wrapping the destination in a [`std::io::BufWriter`].
    pub fn write<W: std::io::Write>(
        &self,
        destination: W,
        buffer_pool: Option<&buffer::Pool>,
    ) -> std::io::Result<()> {
        writer::write(self, destination, buffer_pool)
    }

    /// Writes the binary contents of the module to a file, automatically wrapping it in a [`std::io::BufWriter`].
    pub fn write_to_file(
        &self,
        destination: std::fs::File,
        buffer_pool: Option<&buffer::Pool>,
    ) -> std::io::Result<()> {
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

            self.contents
                .insert(RawModule::from_vec(module_buffer.into_vec()))
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
    pub fn parse<R: std::io::Read>(
        source: R,
        buffer_pool: Option<&buffer::Pool>,
    ) -> Result<Self, ParseError> {
        parser::parse(source, buffer_pool)
    }

    /// Parses a module contained a byte slice.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use sailar::module::{FormatVersion, Module};
    /// let contents = &[
    ///     b'S', b'A', b'I', b'L', b'A', b'R',
    ///     0, 12, // Format version
    ///     0, // Length size
    ///     8, // Header size
    ///     4, // Module name length
    ///     b'T', b'e', b's', b't', // Module name
    ///     2, // Module version length
    ///     1, 0, // Module Version
    ///     0, // Type signatures size
    ///     0, // Function signatures size
    /// ];
    ///
    /// let module = Module::from_slice(contents, None)?;
    /// assert_eq!(module.format_version(), FormatVersion::MINIMUM_SUPPORTED);
    /// assert_eq!(module.name(), sailar::Id::from_str("Test")?);
    /// assert_eq!(module.version(), &[ 1, 0 ]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[inline]
    pub fn from_slice(
        bytes: &[u8],
        buffer_pool: Option<&buffer::Pool>,
    ) -> Result<Self, ParseError> {
        Self::parse(bytes, buffer_pool)
    }

    /// Parses a module contained in the byte vector, and stores the bytes alongside the parsed [`Module`].
    ///
    /// The byte vector can be retrieved again by calling [`Module::raw_contents()`].
    pub fn from_vec(
        bytes: Vec<u8>,
        buffer_pool: Option<&buffer::Pool>,
    ) -> Result<Self, ParseError> {
        let mut module = Self::from_slice(&bytes, buffer_pool)?;
        module.contents = Some(crate::binary::RawModule::from_vec(bytes));
        Ok(module)
    }

    /// Adds a function definition or import to this module.
    pub fn add_function(&mut self, symbol: Identifier, signature: Arc<function::Signature>, kind: function::Kind) -> Arc<function::Function> {
        // TODO: Add the function and the symbol to some sort of lookup
        // TODO: Add the function and the kind to some sort of lookup/Vec that keeps track of which functions have been defined.
        // Result<Arc<function::Function>, DuplicateSymbolError>
    }
}

impl TryFrom<Vec<u8>> for Module {
    type Error = ParseError;

    #[inline]
    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Self::from_vec(bytes, None)
    }
}
