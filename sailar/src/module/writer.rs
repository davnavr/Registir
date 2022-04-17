//! Code for emitting SAILAR modules.

use crate::binary::{self, buffer};
use crate::function;
use crate::module::Module;
use crate::type_system;
use std::io::Write;

type Result = std::io::Result<()>;

mod output {
    use super::Result;
    use crate::binary::LengthSize;
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

        #[must_use]
        pub fn write_length(&mut self, length: usize) -> Result {
            (self.length_writer)(self, length)
        }

        #[must_use]
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

        #[must_use]
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
            let mut $wrapper_name = wrap_rented_buffer!($buffer_name);
        };
    }

    {
        rent_default_buffer_wrapped!(header_buffer, header);
        let name = module.name.as_bytes();
        header.write_length(name.len())?;
        header.write_all(name)?;
        header.write_length(module.version.len() * usize::from(length_size.byte_count()))?;
        header.write_many(module.version.iter(), |numbers, version| numbers.write_length(*version))?;

        out.write_length(header.len())?;
        out.write_all(&header)?;
    }

    // TODO: Could go lazy route and just emit to function signature buffer directly and increment an index counter instead of slowing down for lookups.
    let mut function_signature_lookup = lookup::IndexMap::<&function::Signature>::default();
    let mut definitions_buffer = rent_default_buffer!();

    {
        let mut definitions = wrap_rented_buffer!(definitions_buffer);

        {
            let function_definitions = module.function_definitions();
            rent_default_buffer_wrapped!(functions_buffer, functions);
            functions.write_many(function_definitions, |def, current| {
                let body = current.definition().body();
                let mut flags = current.definition().is_export().flag();
                flags |= body.flag();
                def.write_all(std::slice::from_ref(&flags))?;
                def.write_length(function_signature_lookup.get_or_insert(current.function().signature()))?;
                let symbol = current.function().symbol();
                def.write_length(symbol.len())?;
                def.write_all(symbol.as_bytes())?;

                match current.definition().body() {
                    function::Body::Defined(defined) => todo!("create a block lookup"),
                    function::Body::Foreign(foreign) => todo!("create an identifier lookup"),
                }
            })?;

            definitions.write_buffer_and_count(function_definitions.len(), &functions)?;
        }
    }

    let mut type_signature_lookup = lookup::IndexMap::<&type_system::Any>::default();

    let function_signature_count = function_signature_lookup.len();
    let mut function_signature_buffer = rent_default_buffer!();
    wrap_rented_buffer!(function_signature_buffer).write_many(function_signature_lookup.into_keys(), |sig, current| {
        todo!("write function signature")
    })?;

    // TODO: Write identifiers

    {
        let type_signature_count = type_signature_lookup.len();
        rent_default_buffer_wrapped!(type_signature_buffer, signatures);
        signatures.write_many(type_signature_lookup.into_keys(), |sig, current| {
            todo!("write type signature")
        })?;

        out.write_buffer_and_count(type_signature_count, &signatures)?;
    }

    out.write_buffer_and_count(function_signature_count, &function_signature_buffer)?;

    //out.write_all(&definitions_buffer);

    out.flush()
}
