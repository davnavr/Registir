//! Contains sample SAILAR programs.

use sailar::builder::Builder;
use sailar::identifier::{Id, Identifier};
use sailar::index;
use sailar::instruction::{self, Instruction};
use sailar::num::VarU28;
use sailar::record;
use sailar::signature;

/// Produces a sample program containing an entry point function that simple returns with the specified exit code.
pub fn exit_with(name: Identifier, exit_code: u32) -> Builder<'static> {
    let mut builder = Builder::new();

    builder.add_record(record::MetadataField::ModuleIdentifier(record::ModuleIdentifier::new_owned(
        name,
        vec![VarU28::from_u8(1), VarU28::from_u8(1)],
    )));

    let integer_type = {
        builder.add_record(signature::Type::from(signature::IntegerType::S32));
        index::TypeSignature::from(0)
    };

    let main_signature = {
        builder.add_record(signature::Function::new([].as_slice(), vec![integer_type]));
        index::FunctionSignature::from(0)
    };

    let main_code = {
        let instructions = vec![Instruction::Ret(
            vec![instruction::ConstantInteger::I32(exit_code.to_le_bytes()).into()].into_boxed_slice(),
        )];

        builder.add_record(record::CodeBlock::new(
            [].as_slice(),
            vec![integer_type],
            [].as_slice(),
            instructions,
        ));
        index::CodeBlock::from(0)
    };

    builder.add_record(record::FunctionDefinition::new(
        main_signature,
        record::FunctionBody::Definition(main_code),
    ));

    let entry_point = {
        builder.add_record(record::FunctionInstantiation::from_template(
            record::Export::new_export_borrowed(Id::try_from_str("main").unwrap()),
            index::FunctionTemplate::from(0),
        ));
        index::FunctionInstantiation::from(0)
    };

    builder.add_record(record::MetadataField::EntryPoint(entry_point));

    builder
}
