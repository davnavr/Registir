//! Code for emitting SAILAR modules.

use crate::binary::{self, buffer};
use crate::block;
use crate::function;
use crate::identifier::Id;
use crate::instruction_set::{self, Instruction};
use crate::module::Module;
use crate::type_system;
use std::io::Write;

type Result = std::io::Result<()>;

mod output {
    use super::Result;
    use crate::binary::LengthSize;
    use crate::identifier::Id;
    use std::io::Write;

    type LengthIntegerWriter<W> = fn(&mut Wrapper<W>, usize) -> Result;

    pub struct Wrapper<W> {
        destination: W,
        length_writer: LengthIntegerWriter<W>,
    }

    macro_rules! length_writer {
        ($name: ident, $integer_type: ty) => {
            impl<W: Write> Wrapper<W> {
                fn $name(&mut self, length: usize) -> Result {
                    match <$integer_type>::try_from(length) {
                        Ok(value) => self.destination.write_all(&value.to_le_bytes()),
                        Err(_) => unreachable!(
                            "attempt to write invalid length value {}, but maximum was {}",
                            length,
                            <$integer_type>::MAX
                        ),
                    }
                }
            }
        };
    }

    length_writer!(length_writer_one, u8);
    length_writer!(length_writer_two, u16);
    length_writer!(length_writer_four, u32);

    impl<W: Write> Wrapper<W> {
        pub fn new(destination: W, length_size: LengthSize) -> Self {
            Self {
                destination,
                length_writer: match length_size {
                    LengthSize::One => Self::length_writer_one,
                    LengthSize::Two => Self::length_writer_two,
                    LengthSize::Four => Self::length_writer_four,
                },
            }
        }

        pub fn write_length(&mut self, length: usize) -> Result {
            (self.length_writer)(self, length)
        }

        pub fn write_identifier(&mut self, identifier: &Id) -> Result {
            self.write_length(identifier.len())?;
            self.destination.write_all(identifier.as_bytes())
        }

        pub fn write_many<T, I: std::iter::IntoIterator<Item = T>, O: FnMut(&mut Self, T) -> Result>(
            &mut self,
            items: I,
            mut writer: O,
        ) -> Result {
            for item in items.into_iter() {
                writer(self, item)?;
            }
            Ok(())
        }

        /// Writes a byte size, count, and the contents of a buffer to the output. If the buffer is empty, simply writes a
        /// length integer with a value of `0`.
        pub fn write_buffer_and_count(&mut self, count: usize, buffer: &[u8]) -> Result {
            if count > 0 {
                self.write_length(buffer.len())?;
                self.write_length(count)?;
                self.destination.write_all(buffer)
            } else {
                self.write_length(0)
            }
        }
    }

    impl<W> std::ops::Deref for Wrapper<W> {
        type Target = W;

        fn deref(&self) -> &W {
            &self.destination
        }
    }

    impl<W> std::ops::DerefMut for Wrapper<W> {
        fn deref_mut(&mut self) -> &mut W {
            &mut self.destination
        }
    }
}

mod lookup {
    use rustc_hash::FxHashMap;
    use std::collections::hash_map::Entry;

    // TODO: Checking the references instead of doing a slow Eq operation may be faster.
    //pub struct Key<'a, K>(&'a K);

    #[derive(Debug)]
    pub struct IndexMap<K> {
        lookup: FxHashMap<K, usize>,
    }

    impl<K: Eq + std::hash::Hash> IndexMap<K> {
        pub fn get_or_insert(&mut self, key: K) -> usize {
            let next_index = self.lookup.len();
            match self.lookup.entry(key) {
                Entry::Occupied(occupied) => *occupied.get(),
                Entry::Vacant(vacant) => *vacant.insert(next_index),
            }
        }

        pub fn len(&self) -> usize {
            self.lookup.len()
        }

        pub fn into_keys(self) -> impl std::iter::ExactSizeIterator<Item = K> {
            self.lookup.into_keys()
        }
    }

    impl<K> Default for IndexMap<K> {
        fn default() -> Self {
            Self {
                lookup: FxHashMap::default(),
            }
        }
    }
}

pub fn write<W: Write>(module: &Module, destination: W, buffer_pool: Option<&buffer::Pool>) -> Result {
    use output::Wrapper;

    let length_size = module.length_size;
    let mut out = Wrapper::new(destination, length_size);
    let buffer_pool = buffer::Pool::existing_or_default(buffer_pool);

    {
        out.write_all(binary::MAGIC.as_slice())?;
        let format_version = &module.format_version;
        out.write_all(&[format_version.major, format_version.minor, length_size.into()])?;
    }

    macro_rules! rent_default_buffer {
        () => {
            buffer_pool.rent_with_capacity(32)
        };
    }

    macro_rules! wrap_rented_buffer {
        ($buffer: expr) => {
            Wrapper::new($buffer.as_mut_vec(), length_size)
        };
    }

    macro_rules! rent_default_buffer_wrapped {
        ($buffer_name: ident, $wrapper_name: ident) => {
            let mut $buffer_name = rent_default_buffer!();
            #[allow(unused_mut)]
            let mut $wrapper_name = wrap_rented_buffer!($buffer_name);
        };
    }

    {
        rent_default_buffer_wrapped!(header_buffer, header);
        header.write_identifier(module.name.as_id())?;
        header.write_length(module.version.len() * usize::from(length_size.byte_count()))?;
        header.write_many(module.version.iter(), |numbers, version| numbers.write_length(*version))?;

        out.write_length(header.len())?;
        out.write_all(&header)?;
    }

    // TODO: Could go lazy route and just emit to function signature buffer directly and increment an index counter instead of slowing down for lookups.
    let mut identifier_lookup = lookup::IndexMap::<&Id>::default();
    let mut code_block_lookup = lookup::IndexMap::<&block::Block>::default();
    let mut function_signature_lookup = lookup::IndexMap::<&function::Signature>::default();
    let mut definitions_buffer = rent_default_buffer!();

    {
        let mut definitions = wrap_rented_buffer!(definitions_buffer);

        {
            let function_definitions = module.function_definitions();
            rent_default_buffer_wrapped!(functions_buffer, functions);
            functions.write_many(function_definitions, |def, current| {
                let body = current.definition().body();
                def.write_all(&[current.definition().flags().bits()])?;
                def.write_length(function_signature_lookup.get_or_insert(current.function().signature()))?;
                def.write_identifier(current.function().symbol())?;

                match current.definition().body() {
                    function::Body::Defined(defined) => def.write_length(code_block_lookup.get_or_insert(defined)),
                    function::Body::Foreign(foreign) => {
                        def.write_length(identifier_lookup.get_or_insert(foreign.library_name().as_id()))?;
                        def.write_identifier(foreign.entry_point_name())
                    }
                }
            })?;

            definitions.write_length(function_definitions.len())?;
            definitions.write_all(&functions)?;
        }

        definitions.write_length(0)?; // TODO: Write struct definitions
        definitions.write_length(0)?; // TODO: Write global definitions
        definitions.write_length(0)?; // TODO: Write exception class definitions
        definitions.write_length(0)?; // TODO: Write annotation class definitions
    }

    let mut type_signature_lookup = lookup::IndexMap::<&type_system::Any>::default();

    let function_signature_count = function_signature_lookup.len();
    let mut function_signature_buffer = rent_default_buffer!();
    wrap_rented_buffer!(function_signature_buffer).write_many(function_signature_lookup.into_keys(), |sig, current| {
        sig.write_length(current.result_types().len())?;
        sig.write_length(current.parameter_types().len())?;
        let all_types = current.result_types().iter().chain(current.parameter_types());
        sig.write_many(all_types, |types, function_type| {
            types.write_length(type_signature_lookup.get_or_insert(function_type))
        })
    })?;

    let code_block_count = code_block_lookup.len();
    let mut code_block_buffer = rent_default_buffer!();
    wrap_rented_buffer!(code_block_buffer).write_many(code_block_lookup.into_keys(), |block, current| {
        block.write_length(current.input_types().len())?;
        block.write_length(current.result_types().len())?;
        block.write_length(current.temporary_types().len())?;

        // TODO: Might be more efficient to emit the type of the register (1 byte) directly, and could fall back to index (1-4 bytes) if needed.
        let register_types = current
            .input_types()
            .iter()
            .chain(current.result_types())
            .chain(current.temporary_types());

        block.write_many(register_types, |indices, register_type| {
            indices.write_length(type_signature_lookup.get_or_insert(register_type))
        })?;

        fn write_value<W: Write>(output: &mut Wrapper<W>, value: &instruction_set::Value) -> Result {
            let flags = value.flags();
            output.write_all(&[flags.bits()])?;
            match value {
                instruction_set::Value::IndexedRegister(index) => output.write_length(*index),
                instruction_set::Value::Constant(instruction_set::Constant::Integer(integer)) => match integer {
                    _ if flags.contains(instruction_set::ValueFlags::INTEGER_EMBEDDED) => Ok(()),
                    instruction_set::ConstantInteger::I8(byte) => output.write_all(&[*byte]),
                    instruction_set::ConstantInteger::I16(ref bytes) => output.write_all(bytes),
                    instruction_set::ConstantInteger::I32(ref bytes) => output.write_all(bytes),
                    instruction_set::ConstantInteger::I64(ref bytes) => output.write_all(bytes),
                },
            }
        }

        rent_default_buffer_wrapped!(instruction_buffer, instructions);
        instructions.write_many(current.instructions().iter(), |body, instruction| {
            body.write_all(&[u8::from(instruction.opcode())])?;
            match instruction {
                Instruction::Nop | Instruction::Break => Ok(()),
                Instruction::Ret(return_values) => {
                    body.write_length(return_values.len())?;
                    body.write_many(return_values.iter(), |values, v| write_value(values, v))
                }
                Instruction::AddI(operation) | Instruction::SubI(operation) | Instruction::MulI(operation) => {
                    body.write_all(&[u8::from(operation.overflow_behavior())])?;
                    write_value(body, operation.x_value())?;
                    write_value(body, operation.y_value())
                }
                bad => todo!("attempt to write unsupported instruction {:?}", bad),
            }
        })?;

        block.write_buffer_and_count(current.instructions().len(), &instructions)
    })?;

    {
        let identifier_count = identifier_lookup.len();
        rent_default_buffer_wrapped!(identifier_buffer, identifiers);
        identifiers.write_many(identifier_lookup.into_keys(), |ids, i| ids.write_identifier(i))?;
        out.write_buffer_and_count(identifier_count, &identifiers)?;
    }

    {
        let type_signature_count = type_signature_lookup.len();
        rent_default_buffer_wrapped!(type_signature_buffer, signatures);
        signatures.write_many(type_signature_lookup.into_keys(), |sig, current| match current {
            type_system::Any::Primitive(_) => sig.write_all(&[u8::from(current.tag())]),
        })?;

        out.write_buffer_and_count(type_signature_count, &signatures)?;
    }

    out.write_buffer_and_count(function_signature_count, &function_signature_buffer)?;
    out.write_length(0)?; // TODO: Write module data
    out.write_buffer_and_count(code_block_count, &code_block_buffer)?;
    out.write_length(0)?; // TODO: Write module imports
    out.write_all(&definitions_buffer)?;
    out.write_length(0)?; // TODO: Write struct instantiations
    out.write_length(0)?; // TODO: Write function instantiations
                          // TODO: Write entry point
                          // TODO: Write initializer
    out.write_length(0)?; // TODO: Write namespaces
    out.write_length(0)?; // TODO: Write debugging information
    out.flush()
}
