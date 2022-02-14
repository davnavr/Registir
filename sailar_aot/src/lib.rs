use inkwell::context::Context;
use sailar_get::loader;
use std::cell::RefCell;
use std::collections::hash_map;

mod code_gen;
mod error;

pub use error::{Error, Result};

struct ComparableRef<'a, T>(&'a T);

impl<T> std::cmp::PartialEq for ComparableRef<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0 as *const T, other.0 as *const T)
    }
}

impl<T> std::cmp::Eq for ComparableRef<'_, T> {}

impl<T> std::hash::Hash for ComparableRef<'_, T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hasher::write_usize(state, self.0 as *const T as usize)
    }
}

struct TypeLookup<'c, 'l> {
    context: &'c Context,
    loader: &'l loader::Loader<'l>,
    type_lookup: RefCell<
        hash_map::HashMap<&'l loader::TypeSignature<'l>, inkwell::types::BasicTypeEnum<'c>>,
    >,
    function_lookup: RefCell<
        hash_map::HashMap<
            ComparableRef<'l, loader::FunctionSignature<'l>>,
            inkwell::types::FunctionType<'c>,
        >,
    >,
}

impl<'c, 'l> TypeLookup<'c, 'l> {
    fn get_type(
        &self,
        signature: &'l loader::TypeSignature<'l>,
    ) -> inkwell::types::BasicTypeEnum<'c> {
        use inkwell::types::BasicTypeEnum;
        use sailar::format::type_system as sailar_types;

        match self.type_lookup.borrow_mut().entry(signature) {
            hash_map::Entry::Occupied(occupied) => *occupied.get(),
            hash_map::Entry::Vacant(vacant) => *vacant.insert(match signature.as_raw() {
                sailar_types::Any::Primitive(sailar_types::Primitive::Int(int)) => {
                    BasicTypeEnum::IntType(match int {
                        sailar_types::Int::Fixed(fixed_int) => match fixed_int {
                            sailar_types::FixedInt::U8 | sailar_types::FixedInt::S8 => {
                                self.context.i8_type()
                            }
                            sailar_types::FixedInt::U16 | sailar_types::FixedInt::S16 => {
                                self.context.i16_type()
                            }
                            sailar_types::FixedInt::U32 | sailar_types::FixedInt::S32 => {
                                self.context.i32_type()
                            }
                            sailar_types::FixedInt::U64 | sailar_types::FixedInt::S64 => {
                                self.context.i64_type()
                            }
                        },
                        sailar_types::Int::SNative | sailar_types::Int::UNative => self
                            .context
                            .custom_width_int_type(u32::from(self.loader.pointer_size().get())),
                    })
                }
                sailar_types::Any::Primitive(sailar_types::Primitive::Real(real)) => {
                    BasicTypeEnum::FloatType(match real {
                        sailar_types::Real::F32 => self.context.f32_type(),
                        sailar_types::Real::F64 => self.context.f64_type(),
                    })
                }
                bad => todo!("unsupported type {}", bad),
            }),
        }
    }

    fn get_function(
        &self,
        signature: &'l loader::FunctionSignature<'l>,
    ) -> inkwell::types::FunctionType<'c> {
        // TODO: Make custom enum that is like AnyTypeEnum, but excludes FunctionType.
        use inkwell::types::{AnyType, AnyTypeEnum};

        match self
            .function_lookup
            .borrow_mut()
            .entry(ComparableRef(signature))
        {
            hash_map::Entry::Occupied(occupied) => *occupied.get(),
            hash_map::Entry::Vacant(vacant) => *vacant.insert({
                let return_type = match signature.return_types().first() {
                    Some(return_type) if signature.return_types().len() == 1 => {
                        self.get_type(return_type).as_any_type_enum()
                    }
                    Some(_) => todo!(
                        "multiple return types {} are not yet supported",
                        signature.return_types().len()
                    ),
                    None => AnyTypeEnum::VoidType(self.context.void_type()),
                };

                let parameter_types = signature
                    .parameter_types()
                    .iter()
                    .map(|parameter_type| {
                        inkwell::types::BasicMetadataTypeEnum::from(self.get_type(parameter_type))
                    })
                    .collect::<Vec<_>>();

                match return_type {
                    AnyTypeEnum::ArrayType(array_type) => {
                        array_type.fn_type(parameter_types.as_slice(), false)
                    }
                    AnyTypeEnum::FloatType(float_type) => {
                        float_type.fn_type(parameter_types.as_slice(), false)
                    }
                    AnyTypeEnum::FunctionType(_) => {
                        todo!("cannot return function type, return a function pointer instead")
                    }
                    AnyTypeEnum::IntType(integer_type) => {
                        integer_type.fn_type(parameter_types.as_slice(), false)
                    }
                    AnyTypeEnum::PointerType(pointer_type) => {
                        pointer_type.fn_type(parameter_types.as_slice(), false)
                    }
                    AnyTypeEnum::StructType(struct_type) => {
                        struct_type.fn_type(parameter_types.as_slice(), false)
                    }
                    AnyTypeEnum::VectorType(vector_type) => {
                        vector_type.fn_type(parameter_types.as_slice(), false)
                    }
                    AnyTypeEnum::VoidType(void_type) => {
                        void_type.fn_type(parameter_types.as_slice(), false)
                    }
                }
            }),
        }
    }
}

struct NameLookup<'l> {
    module_prefixes: hash_map::HashMap<ComparableRef<'l, loader::Module<'l>>, String>,
}

impl<'l> NameLookup<'l> {
    fn get(&mut self, function: &'l loader::Function<'l>) -> Result<String> {
        let module_name = self
            .module_prefixes
            .entry(ComparableRef(function.declaring_module()))
            .or_insert_with_key(|module| {
                use std::fmt::Write;

                let identifier = module.0.identifier();
                let mut name = identifier.name.to_string();

                for number in identifier.version.0.iter() {
                    write!(&mut name, "_{}", number).unwrap();
                }

                name
            });

        Ok(format!("{}_{}", module_name, function.symbol()?))
    }
}

/// Compiles the specified SAILAR module with its dependencies into an LLVM module.
pub fn compile<'c>(
    application: sailar::format::Module,
    resolver: &mut dyn loader::ReferenceResolver,
    context: &'c Context,
    target: &inkwell::targets::TargetMachine,
) -> Result<inkwell::module::Module<'c>> {
    let mut loader = None;
    let (loader, application) = loader::Loader::initialize(
        &mut loader,
        loader::PointerSize::try_from(
            std::num::NonZeroU32::try_from(target.get_target_data().get_pointer_byte_size(None))
                .unwrap(),
        )
        .unwrap(),
        resolver,
        application,
    );

    let module = context.create_module(&application.identifier().name);

    let mut function_lookup = hash_map::HashMap::with_capacity(
        application.source().definitions.0.defined_functions.0.len(),
    );

    let type_lookup = TypeLookup {
        context,
        loader,
        type_lookup: RefCell::default(),
        function_lookup: RefCell::default(),
    };

    // Contains the functions that did not have their LLVM bitcode generated.
    // TODO: Include entry point function
    let mut undefined_functions = application
        .iter_defined_functions()
        .filter(|function| function.is_export())
        .collect::<Vec<_>>();

    if let Some(entry_point_function) = application.entry_point()? {
        undefined_functions.push(entry_point_function);

        // TODO: Define a "main" function.
    }

    let mut function_names = NameLookup {
        module_prefixes: hash_map::HashMap::new(),
    };

    let code_builder = context.create_builder();
    let code = code_gen::Cache::new(&code_builder);

    while let Some(function) = undefined_functions.pop() {
        let definition = match function_lookup.entry(ComparableRef(function)) {
            hash_map::Entry::Occupied(occupied) => *occupied.get(),
            hash_map::Entry::Vacant(vacant) => {
                let defined = module.add_function(
                    &function_names.get(function)?,
                    type_lookup.get_function(function.signature()?),
                    // TODO: Function DECLARATIONS (different from definitions) cannot be marked Private, but being able to mark non-exported functions as Private in LLVM would be nice.
                    Some(inkwell::module::Linkage::External),
                );
                undefined_functions.push(function);
                *vacant.insert(defined)
            }
        };

        if definition.count_basic_blocks() == 0u32 {
            code_gen::generate(context, function, definition, &code)?;
        }
    }

    Ok(module)
}