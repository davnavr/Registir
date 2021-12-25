use crate::ast;
use registir::format;

mod declare;
mod indexed;

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Declaration {
    Code,
    TypeDefinition,
    MethodDefinition,
    MethodBody,
    Module,
    Format,
    Name,
    Namespace,
    /// A module or format version.
    Version,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum NameError {
    Duplicate,
    Empty,
    Missing,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Error {
    DuplicateNamedDeclaration(ast::GlobalSymbol, Declaration),
    DuplicateDeclaration(ast::Position, Declaration),
    DuplicateModifier(ast::Position, &'static str),
    InvalidNameDeclaration(ast::Position, Declaration, NameError),
    /// Used when a declaration that is required to be present, such as the module header, was not defined.
    MissingDeclaration(Option<ast::Position>, Declaration),
    UndefinedGlobalSymbol(ast::GlobalSymbol, Declaration),
}

impl Error {
    pub fn position(&self) -> Option<&ast::Position> {
        match self {
            Self::DuplicateNamedDeclaration(
                ast::GlobalSymbol(ast::Positioned { position, .. }),
                _,
            )
            | Self::UndefinedGlobalSymbol(ast::GlobalSymbol(ast::Positioned { position, .. }), _)
            | Self::DuplicateDeclaration(position, _)
            | Self::DuplicateModifier(position, _)
            | Self::InvalidNameDeclaration(position, _, _) => Some(position),
            Self::MissingDeclaration(position, _) => position.as_ref(),
        }
    }
}

impl std::fmt::Display for Declaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Name => "name",
            Self::Namespace => "namespace",
            Self::TypeDefinition => "type definition",
            Self::MethodDefinition => "method definition",
            Self::MethodBody => "method body",
            Self::Code => "code",
            Self::Module => "module",
            Self::Format => "format",
            Self::Version => "version",
        })
    }
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Duplicate => "duplicate",
            Self::Missing => "missing",
            Self::Empty => "empty",
        })
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateNamedDeclaration(symbol, declaration) => write!(
                f,
                "a {} declaration corresponding to the symbol @{} already exists",
                declaration, symbol.0.value
            ),
            Self::DuplicateDeclaration(_, declaration) => {
                write!(f, "a {} declaration already exists", declaration)
            }
            Self::DuplicateModifier(_, name) => {
                write!(f, "the {} modifier was already specified", name)
            }
            Self::InvalidNameDeclaration(_, declaration, error) => {
                write!(f, "{} name for {} declaration", error, declaration)
            }
            Self::MissingDeclaration(_, declaration) => {
                write!(f, "missing {} declaration", declaration)
            }
            Self::UndefinedGlobalSymbol(symbol, declaration) => write!(
                f,
                "a {} corresponding to the symbol @{} could not be found",
                declaration, symbol.0.value
            ),
        }
    }
}

fn assemble_module_header(
    errors: &mut Vec<Error>,
    declarations: &[ast::Positioned<ast::ModuleDeclaration>],
) -> Option<format::ModuleHeader> {
    let mut module_name = declare::Once::new(|name: &ast::Positioned<_>, _| {
        Error::InvalidNameDeclaration(name.position, Declaration::Module, NameError::Duplicate)
    });
    let mut module_version = declare::Once::new(|version: ast::Positioned<_>, _| {
        Error::DuplicateDeclaration(version.position, Declaration::Version)
    });

    for node in declarations {
        match &node.value {
            ast::ModuleDeclaration::Name(name) => {
                if let Some(set_name) = module_name.declare(errors, name) {
                    set_name(match format::Identifier::try_from(&name.value.0) {
                        Ok(id) => Some(id),
                        Err(_) => {
                            errors.push(Error::InvalidNameDeclaration(
                                name.position,
                                Declaration::Module,
                                NameError::Empty,
                            ));
                            None
                        }
                    })
                }
            }
            ast::ModuleDeclaration::Version(version) => {
                if let Some(set_version) = module_version.declare(
                    errors,
                    ast::Positioned {
                        value: version,
                        position: node.position,
                    },
                ) {
                    set_version(format::VersionNumbers(
                        format::structures::LengthEncodedVector(
                            version
                                .iter()
                                .map(|n| format::numeric::UInteger(*n))
                                .collect(),
                        ),
                    ))
                }
            }
        }
    }

    match module_name.value().flatten() {
        Some(name) => Some(format::ModuleHeader {
            identifier: format::ModuleIdentifier {
                name,
                version: module_version.value().unwrap_or_default(),
            },
        }),
        None => {
            errors.push(Error::MissingDeclaration(None, Declaration::Name));
            None
        }
    }
}

fn assemble_module_format(
    errors: &mut Vec<Error>,
    declarations: &[ast::Positioned<ast::FormatDeclaration>],
) -> format::FormatVersion {
    fn declare_version<'a>() -> declare::Once<
        'a,
        ast::Position,
        format::numeric::UInteger,
        impl Fn(ast::Position, &format::numeric::UInteger) -> Error,
    > {
        declare::Once::new(|position, _| {
            Error::DuplicateDeclaration(position, Declaration::Version)
        })
    }

    let mut major_version = declare_version();
    let mut minor_version = declare_version();

    for node in declarations {
        match &node.value {
            ast::FormatDeclaration::Major(major) => {
                major_version.declare_and_set(errors, node.position, *major)
            }
            ast::FormatDeclaration::Minor(minor) => {
                minor_version.declare_and_set(errors, node.position, *minor)
            }
        }
    }

    format::FormatVersion {
        major: major_version.value().unwrap_or_default(),
        minor: minor_version.value().unwrap_or_default(),
    }
}

type IdentifierLookup = indexed::Set<format::indices::Identifier, format::Identifier>;

type NamespaceLookup = indexed::Set<format::indices::Namespace, format::Namespace>;

type TypeSignatureLookup =
    indexed::Set<format::indices::TypeSignature, format::type_system::AnyType>;

type MethodSignatureLookup =
    indexed::Set<format::indices::MethodSignature, format::MethodSignature>;

type MethodBodyLookup<'a> =
    indexed::SymbolMap<'a, ast::GlobalSymbol, format::indices::Code, MethodBodyAssembler<'a>>;

type MethodDefinitionLookup<'a> = indexed::SymbolMap<
    'a,
    ast::GlobalSymbol,
    format::indices::MethodDefinition,
    MethodDefinitionAssembler<'a>,
>;

fn visibility_declaration<'a>() -> declare::Once<
    'a,
    ast::Position,
    format::Visibility,
    impl Fn(ast::Position, &format::Visibility) -> Error,
> {
    declare::Once::new(|position, _| Error::DuplicateModifier(position, "visibility"))
}

type NameDeclaration<'a, 'b, E> =
    declare::Once<'a, &'b ast::Positioned<ast::LiteralString>, format::indices::Identifier, E>;

fn name_declaration<'a, 'b>() -> NameDeclaration<
    'a,
    'b,
    impl Fn(&'b ast::Positioned<ast::LiteralString>, &format::indices::Identifier) -> Error,
> {
    declare::Once::new(|node: &ast::Positioned<_>, _| {
        Error::InvalidNameDeclaration(
            node.position,
            Declaration::TypeDefinition,
            NameError::Duplicate,
        )
    })
}

fn add_identifier_from(
    identifiers: &mut IdentifierLookup,
    declarer: Declaration,
    name: &ast::Positioned<ast::LiteralString>,
) -> Result<format::indices::Identifier, Error> {
    format::Identifier::try_from(&name.value.0)
        .map(|id| identifiers.add(id))
        .map_err(|()| Error::InvalidNameDeclaration(name.position, declarer, NameError::Empty))
}

fn declare_name<
    'a,
    'b,
    E: Fn(&'b ast::Positioned<ast::LiteralString>, &format::indices::Identifier) -> Error,
>(
    declaration: &mut NameDeclaration<'a, 'b, E>,
    errors: &mut Vec<Error>,
    identifiers: &mut IdentifierLookup,
    declarer: Declaration,
    name: &'b ast::Positioned<ast::LiteralString>,
) {
    if let Some(setter) = declaration.declare(errors, name) {
        match add_identifier_from(identifiers, declarer, name) {
            Ok(id) => setter(id),
            Err(error) => errors.push(error),
        }
    }
}

fn assemble_type_signature(
    #[allow(unused_mut)] errors: &mut Vec<Error>,
    signature: &ast::Positioned<ast::TypeSignature>,
) -> Option<format::TypeSignature> {
    match signature.value {
        ast::TypeSignature::Primitive(primitive_type) => {
            Some(format::TypeSignature::primitive(primitive_type))
        }
        ast::TypeSignature::Array(_) => todo!("Array types are not yet supported by asmdl"),
    }
}

fn collect_type_signatures(
    errors: &mut Vec<Error>,
    type_signatures: &mut TypeSignatureLookup,
    types: &[ast::Positioned<ast::TypeSignature>],
) -> Option<format::structures::LengthEncodedVector<format::indices::TypeSignature>> {
    let mut collected = Vec::with_capacity(types.len());
    let mut success = true;

    for signature in types {
        if let Some(assembled) = assemble_type_signature(errors, signature) {
            if success {
                collected.push(type_signatures.add(assembled));
            }
        } else {
            success = false;
        }
    }

    if success {
        Some(format::structures::LengthEncodedVector(collected))
    } else {
        None
    }
}

fn assemble_method_signature(
    errors: &mut Vec<Error>,
    type_signatures: &mut TypeSignatureLookup,
    parameter_types: &[ast::Positioned<ast::TypeSignature>],
    return_types: &[ast::Positioned<ast::TypeSignature>],
) -> Option<format::MethodSignature> {
    let parameters_signature = collect_type_signatures(errors, type_signatures, parameter_types);
    let return_signature = collect_type_signatures(errors, type_signatures, return_types);
    Some(format::MethodSignature {
        return_types: return_signature?,
        parameter_types: parameters_signature?,
    })
}

#[derive(Debug)]
struct MethodBodyAssembler<'a> {
    declarations: &'a [ast::Positioned<ast::CodeDeclaration>],
}

#[derive(Debug)]
struct MethodDefinitionAssembler<'a> {
    origin: ast::Position,
    owner: format::indices::TypeDefinition,
    parameter_types: &'a [ast::Positioned<ast::TypeSignature>],
    return_types: &'a [ast::Positioned<ast::TypeSignature>],
    modifiers: &'a [ast::Positioned<ast::MethodModifier>],
    declarations: &'a [ast::Positioned<ast::MethodDeclaration>],
}

impl<'a> MethodDefinitionAssembler<'a> {
    fn assemble(
        &self,
        errors: &mut Vec<Error>,
        identifiers: &mut IdentifierLookup,
        type_signatures: &mut TypeSignatureLookup,
        method_signatures: &mut MethodSignatureLookup,
        method_bodies: &mut MethodBodyLookup,
    ) -> Option<format::Method> {
        let mut visibility = visibility_declaration();
        #[allow(unused_mut)]
        let mut flags = format::MethodFlags::default();

        for modifier in self.modifiers {
            match &modifier.value {
                ast::MethodModifier::Public => visibility.declare_and_set(
                    errors,
                    modifier.position,
                    format::Visibility::Public,
                ),
                ast::MethodModifier::Private => visibility.declare_and_set(
                    errors,
                    modifier.position,
                    format::Visibility::Private,
                ),
                ast::MethodModifier::Instance => todo!(),
                ast::MethodModifier::Initializer => todo!(),
            }
        }

        let mut method_name = name_declaration();
        let mut method_body = declare::Once::new(|position, _| {
            Error::DuplicateDeclaration(position, Declaration::MethodBody)
        });

        for declaration in self.declarations {
            match &declaration.value {
                ast::MethodDeclaration::Name(name) => declare_name(
                    &mut method_name,
                    errors,
                    identifiers,
                    Declaration::MethodDefinition,
                    name,
                ),
                ast::MethodDeclaration::Body(body) => {
                    if let Some(set_body) = method_body.declare(errors, declaration.position) {
                        set_body(match body {
                            ast::MethodBodyDeclaration::Defined(name) => {
                                match method_bodies.index_of(name) {
                                    Some(index) => Some(format::MethodBody::Defined(index)),
                                    None => {
                                        errors.push(Error::UndefinedGlobalSymbol(
                                            name.clone(),
                                            Declaration::MethodBody,
                                        ));
                                        None
                                    }
                                }
                            }
                            ast::MethodBodyDeclaration::External { name, library } => {
                                unimplemented!()
                            }
                        })
                    }
                }
            }
        }

        if !method_name.is_set() {
            errors.push(Error::InvalidNameDeclaration(
                self.origin,
                Declaration::MethodDefinition,
                NameError::Missing,
            ))
        }

        if !method_body.is_set() {
            errors.push(Error::MissingDeclaration(
                Some(self.origin),
                Declaration::MethodBody,
            ))
        }

        if let (Some(name), Some(body)) = (method_name.value(), method_body.value().flatten()) {
            Some(format::Method {
                owner: self.owner,
                name,
                visibility: visibility.value().unwrap_or_default(),
                flags,
                signature: method_signatures.add(assemble_method_signature(
                    errors,
                    type_signatures,
                    self.parameter_types,
                    self.return_types,
                )?),
                body,
            })
        } else {
            None
        }
    }
}

#[derive(Debug)]
struct TypeDefinitionAssembler<'a> {
    origin: ast::Position,
    index: format::indices::TypeDefinition,
    modifiers: &'a [ast::Positioned<ast::TypeModifier>],
    declarations: &'a [ast::Positioned<ast::TypeDeclaration>],
}

impl<'a> TypeDefinitionAssembler<'a> {
    fn assemble(
        &self,
        errors: &mut Vec<Error>,
        identifiers: &mut IdentifierLookup,
        namespaces: &mut NamespaceLookup,
        methods: &mut MethodDefinitionLookup<'a>,
    ) -> Option<format::Type> {
        let mut visibility = visibility_declaration();
        #[allow(unused_mut)]
        let mut flags = format::TypeFlags::default();

        for modifier in self.modifiers {
            match &modifier.value {
                ast::TypeModifier::Public => visibility.declare_and_set(
                    errors,
                    modifier.position,
                    format::Visibility::Public,
                ),
                ast::TypeModifier::Private => visibility.declare_and_set(
                    errors,
                    modifier.position,
                    format::Visibility::Private,
                ),
            }
        }

        let mut type_name = name_declaration();
        let mut type_namespace = declare::Once::new(|position: ast::Position, _| {
            Error::DuplicateDeclaration(position, Declaration::Namespace)
        });

        #[allow(unused_mut)]
        let mut field_indices = Vec::new();
        let mut method_indices = Vec::new();

        for declaration in self.declarations {
            match &declaration.value {
                ast::TypeDeclaration::Name(name) => declare_name(
                    &mut type_name,
                    errors,
                    identifiers,
                    Declaration::TypeDefinition,
                    name,
                ),
                ast::TypeDeclaration::Namespace(namespace) => {
                    if let Some(set_namespace) =
                        type_namespace.declare(errors, declaration.position)
                    {
                        let mut indices = Vec::with_capacity(namespace.len());
                        let mut success = true;
                        for name in namespace {
                            match add_identifier_from(
                                identifiers,
                                Declaration::TypeDefinition,
                                name,
                            ) {
                                Ok(id) => indices.push(id),
                                Err(error) => {
                                    errors.push(error);
                                    success = false;
                                }
                            }
                        }

                        if success {
                            set_namespace(
                                namespaces.add(format::structures::LengthEncodedVector(indices)),
                            )
                        }
                    }
                }
                ast::TypeDeclaration::Method {
                    symbol,
                    parameter_types,
                    return_types,
                    modifiers,
                    declarations,
                } => {
                    if let Some(index) = methods.try_add(
                        symbol,
                        MethodDefinitionAssembler {
                            origin: declaration.position,
                            owner: self.index,
                            modifiers,
                            declarations,
                            parameter_types,
                            return_types,
                        },
                    ) {
                        method_indices.push(index);
                    }
                }
            }
        }

        if !type_name.is_set() {
            errors.push(Error::InvalidNameDeclaration(
                self.origin,
                Declaration::TypeDefinition,
                NameError::Missing,
            ))
        }

        if !type_namespace.is_set() {
            errors.push(Error::MissingDeclaration(
                Some(self.origin),
                Declaration::Namespace,
            ))
        }

        if let (Some(name), Some(namespace)) = (type_name.value(), type_namespace.value()) {
            Some(format::Type {
                name,
                namespace,
                visibility: visibility.value().unwrap_or_default(),
                flags,
                layout: format::indices::TypeLayout(format::numeric::UInteger(0)), // TODO: Until `.layout` directives are supported, type layouts will be hard coded.
                inherited_types: format::structures::LengthEncodedVector(Vec::new()),
                fields: format::structures::LengthEncodedVector(field_indices),
                methods: format::structures::LengthEncodedVector(method_indices),
                vtable: format::structures::LengthEncodedVector(Vec::new()),
            })
        } else {
            None // Errors were already added
        }
    }
}

pub fn assemble_declarations(
    declarations: &[ast::Positioned<ast::TopLevelDeclaration>],
) -> Result<format::Module, Vec<Error>> {
    let mut errors = Vec::new();
    let mut module_header = None;
    let mut module_format = None;
    let mut identifiers = IdentifierLookup::new();
    let mut namespaces = NamespaceLookup::new();
    let mut type_signatures = TypeSignatureLookup::new();
    let mut method_signatures = MethodSignatureLookup::new();
    let mut method_bodies = MethodBodyLookup::new();
    let mut type_definitions = indexed::SymbolMap::<
        ast::GlobalSymbol,
        format::indices::TypeDefinition,
        TypeDefinitionAssembler,
    >::new();
    let mut method_definitions = MethodDefinitionLookup::new();

    for node in declarations {
        match &node.value {
            ast::TopLevelDeclaration::Module(ref module_nodes) => {
                if module_header.is_none() {
                    module_header = Some(assemble_module_header(&mut errors, module_nodes))
                } else {
                    errors.push(Error::DuplicateDeclaration(
                        node.position,
                        Declaration::Module,
                    ))
                }
            }
            ast::TopLevelDeclaration::Format(ref format_versions) => match module_format {
                None => module_format = Some(assemble_module_format(&mut errors, format_versions)),
                Some(_) => errors.push(Error::DuplicateDeclaration(
                    node.position,
                    Declaration::Format,
                )),
            },
            ast::TopLevelDeclaration::Code {
                ref symbol,
                declarations,
            } => match method_bodies.try_add(symbol, MethodBodyAssembler { declarations }) {
                Some(_) => (),
                None => errors.push(Error::DuplicateNamedDeclaration(
                    symbol.clone(),
                    Declaration::Code,
                )),
            },
            ast::TopLevelDeclaration::Type {
                ref symbol,
                modifiers,
                declarations,
            } => match type_definitions.try_add_with(symbol, |index| TypeDefinitionAssembler {
                origin: node.position,
                index,
                modifiers,
                declarations,
            }) {
                Some(_) => (),
                None => errors.push(Error::DuplicateNamedDeclaration(
                    symbol.clone(),
                    Declaration::TypeDefinition,
                )),
            },
            ref unknown => unimplemented!("{:?}", unknown),
        }
    }

    fn assemble_definitions<D, T, A: FnMut(&D) -> Option<T>>(
        definitions: &[D],
        mut assemble: A,
    ) -> Vec<T> {
        let mut assembled = Vec::with_capacity(definitions.len());
        let mut commit = true;
        for definition in definitions {
            match assemble(definition) {
                Some(result) => {
                    if commit {
                        assembled.push(result)
                    }
                }
                None => commit = false,
            }
        }
        assembled
    }

    let assembled_method_bodies = assemble_definitions(method_bodies.items(), |body| todo!());

    let assembled_type_definitions = assemble_definitions(type_definitions.items(), |definition| {
        definition.assemble(
            &mut errors,
            &mut identifiers,
            &mut namespaces,
            &mut method_definitions,
        )
    });

    let assembled_method_definitions =
        assemble_definitions(method_definitions.items(), |definition| {
            definition.assemble(
                &mut errors,
                &mut identifiers,
                &mut type_signatures,
                &mut method_signatures,
                &mut method_bodies,
            )
        });

    if module_header.is_none() {
        errors.push(Error::MissingDeclaration(None, Declaration::Module))
    }

    if errors.is_empty() {
        Ok(format::Module {
            integer_size: format::numeric::IntegerSize::I4,
            format_version: module_format.unwrap_or_default(),
            // Some(None) would mean that the module header had no name, but an unwrap is safe here as an error should have been generated.
            header: format::structures::ByteLengthEncoded(module_header.flatten().unwrap()),
            // TODO: Add other things
            identifiers: format::structures::ByteLengthEncoded(
                format::structures::LengthEncodedVector(identifiers.take_items()),
            ),
            namespaces: format::structures::ByteLengthEncoded(
                format::structures::LengthEncodedVector(namespaces.take_items()),
            ),
            type_signatures: format::structures::ByteLengthEncoded(
                format::structures::LengthEncodedVector(type_signatures.take_items()),
            ),
            method_signatures: format::structures::ByteLengthEncoded(
                format::structures::LengthEncodedVector(method_signatures.take_items()),
            ),
            method_bodies: format::structures::ByteLengthEncoded(
                format::structures::LengthEncodedVector(assembled_method_bodies),
            ),
            data_arrays: format::structures::ByteLengthEncoded(
                format::structures::LengthEncodedVector(Vec::new()),
            ),
            imports: format::structures::ByteLengthEncoded(format::ModuleImports {
                imported_modules: format::structures::ByteLengthEncoded(
                    format::structures::LengthEncodedVector(Vec::new()),
                ),
                imported_types: format::structures::ByteLengthEncoded(
                    format::structures::LengthEncodedVector(Vec::new()),
                ),
                imported_fields: format::structures::ByteLengthEncoded(
                    format::structures::LengthEncodedVector(Vec::new()),
                ),
                imported_methods: format::structures::ByteLengthEncoded(
                    format::structures::LengthEncodedVector(Vec::new()),
                ),
            }),
            definitions: format::structures::ByteLengthEncoded(format::ModuleDefinitions {
                defined_types: format::structures::ByteLengthEncoded(
                    format::structures::LengthEncodedVector(assembled_type_definitions),
                ),
                defined_fields: format::structures::ByteLengthEncoded(
                    format::structures::LengthEncodedVector(Vec::new()),
                ),
                defined_methods: format::structures::ByteLengthEncoded(
                    format::structures::LengthEncodedVector(assembled_method_definitions),
                ),
            }),
            entry_point: format::structures::ByteLengthEncoded(None),
            type_layouts: format::structures::ByteLengthEncoded(
                format::structures::LengthEncodedVector(vec![format::TypeLayout::Unspecified]),
            ),
        })
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use crate::{assembler, parser};
    use registir::format;

    #[test]
    fn module_header_test() {
        let declarations = parser::parse(".module { .name \"Test\"; .version 1 2 3; };").unwrap();
        let header = assembler::assemble_declarations(&declarations)
            .map(|module| module.header.0)
            .unwrap();

        assert_eq!(
            Ok(header.identifier.name),
            format::Identifier::try_from("Test")
        );
        assert_eq!(
            header.identifier.version,
            format::VersionNumbers::from_iter(vec![1u32, 2, 3].into_iter())
        );
    }
}
