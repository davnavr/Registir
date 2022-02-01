use crate::format;

mod block;
mod code;
mod counter;
mod error;
mod type_signatures;

pub use block::{Builder as Block, Error as InvalidInstruction};
pub use error::{Error, Result};
pub use format::{FormatVersion, Identifier as Name, VersionNumbers};

#[derive(Clone, Copy)]
pub struct Type<'a> {
    // TODO: Include builder ID?
    //builder: &'a (),
    index: format::indices::TypeSignature,
    signature: &'a format::type_system::Any,
}

pub struct TypeSignatures<'a> {
    builder: &'a (),
    signatures: &'a type_signatures::Signatures,
}

pub struct Code<'a> {
    builder: &'a (),
    code: &'a mut code::Code,
    type_signatures: &'a mut type_signatures::Signatures,
}

pub struct CodeDefinitions<'a> {
    builder: &'a (),
    definitions: &'a mut code::Definitions,
    type_signatures: &'a mut type_signatures::Signatures,
}

pub struct Builder {
    builder_identifier: Box<()>,
    module_identifier: format::ModuleIdentifier,
    format_version: FormatVersion,
    code: code::Definitions,
    type_signatures: type_signatures::Signatures,
}

impl Builder {
    pub fn new(name: Name) -> Self {
        Self {
            builder_identifier: Box::new(()),
            module_identifier: format::ModuleIdentifier {
                name,
                version: format::VersionNumbers::default(),
            },
            format_version: FormatVersion::minimum_supported_version().clone(),
            code: code::Definitions::new(),
            type_signatures: type_signatures::Signatures::new(),
        }
    }

    //pub fn format_version_mut

    pub fn set_format_version(&mut self, version: FormatVersion) {
        self.format_version = version;
    }

    pub fn set_module_version(&mut self, version: VersionNumbers) {
        self.module_identifier.version = version;
    }

    pub fn code(&mut self) -> CodeDefinitions<'_> {
        CodeDefinitions {
            builder: &self.builder_identifier,
            definitions: &mut self.code,
            type_signatures: &mut self.type_signatures,
        }
    }

    pub fn type_signatures(&mut self) -> TypeSignatures<'_> {
        TypeSignatures {
            builder: &self.builder_identifier,
            signatures: &mut self.type_signatures,
        }
    }

    // setup may need to be consumed as well.
    pub fn finish(mut self) -> format::Module {
        format::Module {
            integer_size: format::numeric::IntegerSize::I4,
            format_version: self.format_version,
            header: format::LenBytes(format::ModuleHeader {
                identifier: self.module_identifier,
            }),
            identifiers: format::LenBytes(format::LenVec(Vec::new())),
            namespaces: format::LenBytes(format::LenVec(Vec::new())),
            type_signatures: format::LenBytes(format::LenVec(self.type_signatures.build())),
            function_signatures: format::LenBytes(format::LenVec(Vec::new())),
            function_bodies: format::LenBytes(format::LenVec(self.code.build())),
            data: format::LenBytes(format::LenVec(Vec::new())),
            imports: format::LenBytes(format::ModuleImports {
                imported_modules: format::LenBytes(format::LenVec(Vec::new())),
                imported_structs: format::LenBytes(format::LenVec(Vec::new())),
                imported_globals: format::LenBytes(format::LenVec(Vec::new())),
                imported_fields: format::LenBytes(format::LenVec(Vec::new())),
                imported_functions: format::LenBytes(format::LenVec(Vec::new())),
            }),
            definitions: format::LenBytes(format::ModuleDefinitions {
                defined_structs: format::LenBytes(format::LenVec(Vec::new())),
                defined_globals: format::LenBytes(format::LenVec(Vec::new())),
                defined_fields: format::LenBytes(format::LenVec(Vec::new())),
                defined_functions: format::LenBytes(format::LenVec(Vec::new())),
            }),
            struct_layouts: format::LenBytes(format::LenVec(Vec::new())),
            entry_point: format::LenBytes(None),
        }
    }
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Builder")
            .field("module_identifier", &self.module_identifier)
            .field("format_version", &self.format_version)
            .finish()
    }
}
