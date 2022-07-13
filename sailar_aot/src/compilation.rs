//! Module for translating from SAILAR modules to LLVM modules.

use crate::error::{CompilationError, CompilationErrorKind};
use crate::target;
use inkwell::context::Context as LlvmContext;
use inkwell::module::Module as LlvmModule;
use sailar_load::module::Module;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, CompilationError>;

/// Specifies how the name of the output LLVM module is determined.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum OutputName {
    /// The name of the first SAILAR module used as input is used.
    Inferred,
    Specified(String),
}

impl Default for OutputName {
    fn default() -> Self {
        Self::Inferred
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MainFunctionKind {
    /// Indicates that a C style `main` function should be generated that calls the entry point of the SAILAR program.
    ///
    /// ```C
    /// int main(int argc, char* argv[]) {
    ///     // Call to SAILAR entry point is generated here.
    /// }
    /// ```
    CStyle,
}

impl Default for MainFunctionKind {
    fn default() -> Self {
        Self::CStyle
    }
}

/// Represents the inputs of a compilation.
pub struct Inputs<'input> {
    target_platform: Option<target::Platform<'input>>,
    sources: Vec<sailar_load::source::BoxedSource>,
    resolver: Option<sailar_load::resolver::BoxedResolver>,
    output_name: OutputName,
    main_kind: MainFunctionKind,
}

impl<'input> Inputs<'input> {
    pub fn new() -> Self {
        Self {
            target_platform: None,
            sources: Default::default(),
            resolver: None,
            output_name: Default::default(),
            main_kind: Default::default(),
        }
    }

    /// Explicitly includes the specified modules in this compilation.
    pub fn with_modules<S, M>(mut self, modules: M) -> Self
    where
        S: sailar_load::source::Source + 'static,
        S::Error: std::error::Error + 'static,
        M: IntoIterator<Item = S>,
    {
        self.sources.extend(modules.into_iter().map(sailar_load::source::boxed));
        self
    }

    /// Specifies a [`Resolver`] that retrieves imported modules for inclusion in this compilation.
    ///
    /// [`Resolver`]: sailar_load::Resolver
    pub fn module_resolver<R>(self, resolver: R) -> Self
    where
        R: sailar_load::Resolver + Send + 'static,
        R::Error: std::error::Error,
    {
        Self {
            resolver: Some(sailar_load::resolver::boxed(resolver)),
            ..self
        }
    }

    /// Sets the target platform for the compilation.
    pub fn target_platform(mut self, target_platform: target::Platform<'input>) -> Self {
        self.target_platform = Some(target_platform);
        self
    }

    /// Sets the name of the output LLVM module.
    pub fn output_name(self, name: OutputName) -> Self {
        Self {
            output_name: name,
            ..self
        }
    }

    /// Sets the type of `main` function produced for an executable.
    pub fn main_function_kind(self, kind: MainFunctionKind) -> Self {
        Self { main_kind: kind, ..self }
    }

    /// Given the specified input and LLVM context, compiles the SAILAR code into an LLVM module.
    ///
    /// For information about errors that can occur regarding target information, see the documentation for the [`target`]
    /// module.
    pub fn compile_in_context<'context>(self, context: &'context LlvmContext) -> Result<Compilation<'input, 'context>> {
        let target_platform = if let Some(target) = self.target_platform {
            target
        } else {
            target::Platform::host(inkwell::OptimizationLevel::Default)?
        };

        let state = {
            let address_size = sailar_load::state::AddressSize::with_byte_size(
                std::num::NonZeroU16::new(u16::try_from(target_platform.target().data().get_pointer_byte_size(None)).unwrap())
                    .unwrap(),
            );

            let mut builder = sailar_load::state::Builder::new().address_size(address_size);

            if let Some(resolver) = self.resolver {
                builder = builder.resolver(resolver);
            }

            builder.create()
        };

        let input_modules = {
            let mut modules = Vec::with_capacity(self.sources.len());
            for source in self.sources {
                match state
                    .force_load_module(source)
                    .map_err(CompilationErrorKind::ModuleResolution)?
                {
                    Some(module) => modules.push(module),
                    None => todo!("make error for duplicate module"),
                }
            }
            modules.into_boxed_slice()
        };

        let output_module_name;
        let output_module = context.create_module(match self.output_name {
            OutputName::Inferred => input_modules
                .first()
                .and_then(|module| module.module_identifier())
                .map(|id| id.name().as_str())
                .unwrap_or("<unnamed>"),
            OutputName::Specified(name) => {
                output_module_name = name;
                &output_module_name
            }
        });

        output_module.set_triple(target_platform.target().triple());

        let type_cache = crate::signature::Cache::new(context, target_platform.target().data());
        let function_cache = crate::function::Cache::new(&output_module, &type_cache);
        // TODO: Have a helper NameMangling struct and/or trait that also keeps a String buffer to avoid extra allocations.

        input_modules
            .iter()
            .flat_map(|module| module.symbols().iter_functions())
            .filter(|symbol| !symbol.is_private())
            .try_for_each(|symbol| {
                let function = std::ops::Deref::deref(symbol);
                function_cache.get_or_define(function.clone())?;
                Result::Ok(())
            })?;

        // TODO: Add options to either expect only a single entry point in all modules, specify that an entry point in a named module should be used, or to allow any entry point to be picked.
        let mut main_function_choices = {
            let mut choices = Vec::new();
            for module in input_modules.iter() {
                if let Some(main_function) = module.entry_point()? {
                    choices.push((module.clone(), main_function.clone()));
                }
            }
            choices.into_iter()
        };

        let main_function = if main_function_choices.len() <= 1 {
            if let Some((_, main_function)) = main_function_choices.next() {
                Some((main_function.clone(), function_cache.get_or_define(main_function)?))
            } else {
                None
            }
        } else {
            todo!(
                "handle ambigous entry point ({} possible choices)",
                main_function_choices.len()
            );
        };

        let mut transpiler = crate::transpiler::Transpiler::new(&type_cache);
        while let Some((function_instantiation, llvm_function)) = function_cache.next_undefined() {
            transpiler.translate(function_instantiation, llvm_function)?;
        }

        match self.main_kind {
            MainFunctionKind::CStyle => {
                if let Some((main_instantiation, main_value)) = main_function {
                    let main_signature = main_instantiation.signature()?;
                    if !main_signature.parameter_types()?.is_empty() {
                        todo!("handle main function parameters")
                    }

                    // TODO: Indicate if we need to truncate or sign extend the exit code.
                    // TODO: Add support for other integer types for exit code.
                    let has_exit_code; // Set to false if we need to insert an exit code ourselves
                    match main_signature.return_types()? {
                        [] => {
                            has_exit_code = false;
                        }
                        bad => todo!("unsupported return types {:?}", bad),
                    }

                    let actual_entry_point = {
                        let c_int_type = target_platform.c_data_model().int_size.get_llvm_integer_type(context);
                        let entry_point_signature = c_int_type.fn_type(
                            &[
                                // argc
                                c_int_type.into(),
                                // argv
                                target_platform
                                    .c_data_model()
                                    .int_size
                                    .get_llvm_integer_type(context)
                                    .ptr_type(inkwell::AddressSpace::Generic)
                                    .ptr_type(inkwell::AddressSpace::Generic)
                                    .into(),
                            ],
                            false,
                        );

                        output_module.add_function("main", entry_point_signature, None)
                    };

                    let entry_block = context.append_basic_block(actual_entry_point, "");
                    let builder = context.create_builder();

                    builder.position_at_end(entry_block);
                    let result = builder.build_call(main_value, &[], "");

                    if has_exit_code {
                        let exit_code = result.try_as_basic_value().left().unwrap();
                        builder.build_return(Some(&exit_code));
                    } else {
                        builder.build_return(None);
                    }
                }
            }
        }

        Ok(Compilation {
            output_module,
            input_modules,
            target_platform,
        })
    }

    /// Compiles the specified input into an LLVM module, storing a newly crated LLVM [`Context`].
    ///
    /// For more information, see the documentation for [`compile_in_context`].
    ///
    /// [`Context`]: LlvmContext
    /// [`compile_in_context`]: Inputs::compile_in_context
    pub fn compile<'context>(self, context: &'context mut Option<LlvmContext>) -> Result<Compilation<'input, 'context>> {
        let context = Option::insert(context, LlvmContext::create());
        self.compile_in_context(&*context)
    }
}

impl Default for Inputs<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents an LLVM module containing LLVM IR corresponding to one or more SAILAR modules.
#[derive(Debug)]
pub struct Compilation<'input, 'context> {
    output_module: LlvmModule<'context>,
    input_modules: Box<[Arc<Module>]>,
    target_platform: target::Platform<'input>,
}

impl<'input, 'context> Compilation<'input, 'context> {
    /// Compiles the specified `inputs` in the specified LLVM `context`. Alias for [`Inputs::compile_in_context`].
    #[inline]
    pub fn with_inputs(inputs: Inputs<'input>, context: &'context LlvmContext) -> Result<Self> {
        inputs.compile_in_context(context)
    }

    /// Gets the modules used as inputs for this compilation.
    pub fn input_modules(&self) -> &[Arc<Module>] {
        &self.input_modules
    }

    // Gets the LLVM module produced as a result of compilation.
    pub fn output_module(&self) -> &LlvmModule<'context> {
        &self.output_module
    }

    /// Gets the target platform of this compilation.
    pub fn target_platform(&self) -> &target::Platform<'input> {
        &self.target_platform
    }

    /// Produces an assembly or object file containing the compiled code.
    pub fn write_object_code_to_file(
        &self,
        file_type: inkwell::targets::FileType,
        path: &std::path::Path,
    ) -> std::result::Result<(), inkwell::support::LLVMString> {
        self.target_platform
            .target()
            .machine()
            .write_to_file(&self.output_module, file_type, path)
    }
}
