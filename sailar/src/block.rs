//! Manipulation of SAILAR code blocks.

use crate::binary::VarIntSize;
use crate::function;
use crate::instruction_set::{self, Instruction};
use crate::type_system;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::iter::IntoIterator;
use std::sync::Arc;

#[derive(Clone, Debug, thiserror::Error)]
#[error("expected result at index {index} to be of type {expected:?} but got {actual:?}")]
pub struct InvalidResultTypeError {
    index: usize,
    expected: type_system::Any,
    actual: type_system::Any,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ExpectedTypeErrorKind {
    Expected(type_system::Any),
    ExpectedInteger,
}

#[derive(Clone, Debug)]
pub struct ExpectedTypeError {
    actual: type_system::Any,
    kind: ExpectedTypeErrorKind,
}

impl Display for ExpectedTypeError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str("expected ")?;
        match &self.kind {
            ExpectedTypeErrorKind::Expected(expected) => write!(f, "{:?}", expected)?,
            ExpectedTypeErrorKind::ExpectedInteger => f.write_str("integer type")?,
        }
        write!(f, " but got {:?}", self.actual)
    }
}

impl std::error::Error for ExpectedTypeError {}

#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    #[error("code blocks must not be empty")]
    EmptyBlock,
    #[error(transparent)]
    InvalidResultType(#[from] InvalidResultTypeError),
    #[error("expected {expected} results but got {actual}")]
    ResultCountMismatch { expected: usize, actual: usize },
    #[error(transparent)]
    ExpectedType(#[from] ExpectedTypeError),
    #[error("expected {expected} arguments but got {actual}")]
    ArgumentCountMismatch { expected: usize, actual: usize },
    #[error("expected argument at index {index} to be of type {expected:?}, but got {actual:?}")]
    ArgumentTypeMismatch {
        expected: type_system::Any,
        actual: type_system::Any,
        index: usize,
    },
}

pub type ValidationResult<T> = Result<T, Box<ValidationError>>;

macro_rules! fail {
    ($error: expr) => {
        return Err(Box::new(ValidationError::from($error)))
    };
}

/// Represents an input register, which contains a value passed as an input to a block.
#[derive(Debug, Eq, PartialEq)]
pub struct Input {
    index: usize,
    value_type: type_system::Any,
}

impl Input {
    #[inline]
    fn value_type(&self) -> &type_system::Any {
        &self.value_type
    }
}

/// Represents a temporary register, which contains the result of executing an instruction.
#[derive(Debug, Eq, PartialEq)]
pub struct Temporary {
    index: usize,
    value_type: type_system::Any,
}

impl Temporary {
    #[inline]
    fn value_type(&self) -> &type_system::Any {
        &self.value_type
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ConstantInteger {
    U8(u8),
    S8(i8),
    U16(u16),
    S16(i16),
    U32(u32),
    S32(i32),
    U64(u64),
    S64(i64),
}

impl ConstantInteger {
    pub fn value_type(&self) -> &'static type_system::Any {
        macro_rules! constant_type {
            ($name: ident) => {
                &type_system::Any::Primitive(type_system::Primitive::Int(type_system::Int::Fixed(
                    type_system::FixedInt::$name,
                )))
            };
        }

        match self {
            Self::U8(_) => constant_type!(U8),
            Self::S8(_) => constant_type!(S8),
            Self::U16(_) => constant_type!(U16),
            Self::S16(_) => constant_type!(S16),
            Self::U32(_) => constant_type!(U32),
            Self::S32(_) => constant_type!(S32),
            Self::U64(_) => constant_type!(U64),
            Self::S64(_) => constant_type!(S64),
        }
    }
}

impl From<&ConstantInteger> for instruction_set::ConstantInteger {
    fn from(constant: &ConstantInteger) -> Self {
        match constant {
            ConstantInteger::U8(value) => Self::I8(*value),
            ConstantInteger::S8(value) => Self::I8(*value as u8),
            ConstantInteger::U16(value) => Self::I16(value.to_le_bytes()),
            ConstantInteger::S16(value) => Self::I16(value.to_le_bytes()),
            ConstantInteger::U32(value) => Self::I32(value.to_le_bytes()),
            ConstantInteger::S32(value) => Self::I32(value.to_le_bytes()),
            ConstantInteger::U64(value) => Self::I64(value.to_le_bytes()),
            ConstantInteger::S64(value) => Self::I64(value.to_le_bytes()),
        }
    }
}

impl From<&ConstantInteger> for instruction_set::Constant {
    #[inline]
    fn from(constant: &ConstantInteger) -> Self {
        Self::Integer(constant.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Constant {
    Integer(ConstantInteger),
}

impl Constant {
    pub fn value_type(&self) -> &type_system::Any {
        match self {
            Self::Integer(integer) => integer.value_type(),
        }
    }
}

impl From<&Constant> for instruction_set::Constant {
    fn from(constant: &Constant) -> Self {
        match constant {
            Constant::Integer(integer) => integer.into(),
        }
    }
}

impl From<&Constant> for instruction_set::Value {
    #[inline]
    fn from(constant: &Constant) -> Self {
        Self::Constant(constant.into())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Value<'r> {
    Constant(Constant),
    Temporary(&'r Temporary),
    Input(&'r Input),
}

impl Value<'_> {
    fn to_value(&self, input_count: usize) -> instruction_set::Value {
        match self {
            Self::Constant(constant) => constant.into(),
            Self::Temporary(temporary) => instruction_set::Value::IndexedRegister(temporary.index + input_count),
            Self::Input(input) => instruction_set::Value::IndexedRegister(input.index),
        }
    }
}

crate::enum_case_from_impl!(Value<'_>, Constant, Constant);

impl<'r> From<&'r Temporary> for Value<'r> {
    #[inline]
    fn from(temporary: &'r Temporary) -> Self {
        Self::Temporary(temporary)
    }
}

impl<'r> From<&'r Input> for Value<'r> {
    #[inline]
    fn from(input: &'r Input) -> Self {
        Self::Input(input)
    }
}

impl Value<'_> {
    fn value_type(&self) -> &type_system::Any {
        match self {
            Self::Constant(constant) => constant.value_type(),
            Self::Temporary(temporary) => temporary.value_type(),
            Self::Input(input) => input.value_type(),
        }
    }
}

macro_rules! integer_conversion_impls {
    ($constant_case_name: ident, $integer_type: ty) => {
        impl From<$integer_type> for ConstantInteger {
            #[inline]
            fn from(value: $integer_type) -> Self {
                Self::$constant_case_name(value)
            }
        }

        impl From<$integer_type> for Constant {
            #[inline]
            fn from(value: $integer_type) -> Self {
                Self::Integer(ConstantInteger::from(value))
            }
        }

        impl From<$integer_type> for Value<'_> {
            #[inline]
            fn from(value: $integer_type) -> Self {
                Self::Constant(Constant::from(value))
            }
        }
    };
}

integer_conversion_impls!(U8, u8);
integer_conversion_impls!(S8, i8);
integer_conversion_impls!(U16, u16);
integer_conversion_impls!(S16, i16);
integer_conversion_impls!(U32, u32);
integer_conversion_impls!(S32, i32);
integer_conversion_impls!(U64, u64);
integer_conversion_impls!(S64, i64);

/// Represents the result of an integer arithmetic operation that may have overflowed.
#[derive(Debug)]
pub struct FlaggedOverflow<'r> {
    result: &'r Temporary,
    flag: &'r Temporary,
}

impl<'r> FlaggedOverflow<'r> {
    #[inline]
    pub fn result_register(&self) -> &'r Temporary {
        self.result
    }

    /// The register containing a value indicating if an integer overflow occured.
    #[inline]
    pub fn flag_register(&self) -> &'r Temporary {
        self.flag
    }
}

pub type IntegerArithmeticInstruction = fn(Box<instruction_set::IntegerArithmetic>) -> Instruction;

/// Represents the results of a function call.
pub trait FunctionCall<'r> {
    fn append_result(&mut self, register: &'r Temporary);
}

/// Ignores the results of a function call.
impl FunctionCall<'_> for () {
    fn append_result(&mut self, _: &Temporary) {}
}

impl<'r> FunctionCall<'r> for Vec<&'r Temporary> {
    fn append_result(&mut self, register: &'r Temporary) {
        self.push(register)
    }
}

/// Allows the building of SAILAR code blocks.
pub struct Builder<'b> {
    integer_size: VarIntSize,
    result_types: Box<[type_system::Any]>,
    input_registers: &'b Vec<Input>,
    temporary_registers: &'b elsa::vec::FrozenVec<Box<Temporary>>,
    instructions: &'b mut Vec<Instruction>,
    value_buffer: &'b mut Vec<instruction_set::Value>,
}

impl<'b> Builder<'b> {
    #[inline]
    pub fn result_types(&self) -> &[type_system::Any] {
        &self.result_types
    }

    #[inline]
    pub fn input_registers(&self) -> &'b [Input] {
        self.input_registers
    }

    pub fn emit_nop(&mut self) {
        self.instructions.push(Instruction::Nop);
    }

    pub fn emit_break(&mut self) {
        self.instructions.push(Instruction::Break);
    }

    fn convert_value(&self, value: Value<'b>) -> instruction_set::Value {
        value.to_value(self.input_registers.len())
    }

    fn define_temporary(&mut self, value_type: type_system::Any) -> &'b Temporary {
        let index = self.temporary_registers.len();
        self.temporary_registers.push_get(Box::new(Temporary { index, value_type }))
    }

    fn define_overflow_flag(&mut self) -> &'b Temporary {
        self.define_temporary(type_system::Any::from(type_system::FixedInt::U8))
    }

    fn integer_arithmetic_instruction(
        &mut self,
        overflow_behavior: instruction_set::OverflowBehavior,
        x: Value<'b>,
        y: Value<'b>,
        instruction: IntegerArithmeticInstruction,
    ) -> ValidationResult<&'b Temporary> {
        let x_type = x.value_type().clone();
        let y_type = y.value_type();

        match x_type {
            type_system::Any::Primitive(type_system::Primitive::Int(_)) => (),
            actual => fail!(ExpectedTypeError {
                actual,
                kind: ExpectedTypeErrorKind::ExpectedInteger,
            }),
        }

        if &x_type != y_type {
            fail!(ExpectedTypeError {
                actual: y_type.clone(),
                kind: ExpectedTypeErrorKind::Expected(x_type),
            });
        }

        self.instructions
            .push(instruction(Box::new(instruction_set::IntegerArithmetic::new(
                overflow_behavior,
                self.convert_value(x),
                self.convert_value(y),
            ))));

        Ok(self.define_temporary(x_type))
    }

    fn integer_arithmetic_flagged(
        &mut self,
        overflow_behavior: instruction_set::OverflowBehavior,
        x: Value<'b>,
        y: Value<'b>,
        instruction: IntegerArithmeticInstruction,
    ) -> ValidationResult<FlaggedOverflow<'b>> {
        Ok(FlaggedOverflow {
            result: self.integer_arithmetic_instruction(overflow_behavior, x, y, instruction)?,
            flag: self.define_overflow_flag(),
        })
    }

    /// Emits an instruction that adds two integer values and stores the sum in a temporary register, ignoring any overflows.
    pub fn emit_add<X: Into<Value<'b>>, Y: Into<Value<'b>>>(&mut self, x: X, y: Y) -> ValidationResult<&'b Temporary> {
        self.integer_arithmetic_instruction(
            instruction_set::OverflowBehavior::Ignore,
            x.into(),
            y.into(),
            Instruction::AddI,
        )
    }

    pub fn emit_add_flagged<X: Into<Value<'b>>, Y: Into<Value<'b>>>(
        &mut self,
        x: X,
        y: Y,
    ) -> ValidationResult<FlaggedOverflow<'b>> {
        self.integer_arithmetic_flagged(instruction_set::OverflowBehavior::Flag, x.into(), y.into(), Instruction::AddI)
    }

    /// Emits an instruction that adds two integer values and stores the sum in a temporary register, saturating on overflow.
    pub fn emit_add_saturating<X: Into<Value<'b>>, Y: Into<Value<'b>>>(&mut self, x: X, y: Y) -> ValidationResult<&'b Temporary> {
        self.integer_arithmetic_instruction(
            instruction_set::OverflowBehavior::Saturate,
            x.into(),
            y.into(),
            Instruction::AddI,
        )
    }

    fn finish(mut self) -> ValidationResult<Block> {
        if self.instructions.is_empty() {
            fail!(ValidationError::EmptyBlock);
        }

        let input_types = self.input_registers().iter().map(|input| input.value_type.clone()).collect();
        let temporary_types: Box<_> = self
            .temporary_registers
            .iter()
            .map(|temporary| temporary.value_type.clone())
            .collect();

        self.integer_size.resize_to_fit(temporary_types.len());

        Ok(Block {
            integer_size: self.integer_size,
            result_types: self.result_types,
            input_types,
            temporary_types,
            instructions: self.instructions.clone().into_boxed_slice(),
        })
    }

    /// Emits a terminating instruction that transfers control flow back to the calling function, supplying the specified return values.
    pub fn emit_ret<V: IntoIterator<Item = Value<'b>>>(mut self, values: V) -> ValidationResult<Block> {
        let return_values = values.into_iter();
        let (minimum_return_count, maximum_return_count) = return_values.size_hint();
        self.value_buffer.clear();
        self.value_buffer
            .reserve(maximum_return_count.unwrap_or(minimum_return_count));

        for (index, (value, expected)) in return_values.zip(self.result_types.iter()).enumerate() {
            let actual = value.value_type();
            if actual != expected {
                fail!(InvalidResultTypeError {
                    index,
                    expected: expected.clone(),
                    actual: actual.clone(),
                });
            }

            self.value_buffer.push(value.to_value(self.input_registers.len()));
        }

        let return_value_count = self.value_buffer.len();
        if return_value_count != self.result_types.len() {
            fail!(ValidationError::ResultCountMismatch {
                expected: self.result_types.len(),
                actual: self.value_buffer.len(),
            });
        }

        self.integer_size.resize_to_fit(return_value_count);

        self.instructions
            .push(Instruction::Ret(self.value_buffer.clone().into_boxed_slice()));
        self.finish()
    }

    /// Emits a call to the specified function, providing the specified arguments.
    pub fn emit_call<A: IntoIterator<Item = Value<'b>>, R: FunctionCall<'b>>(
        &mut self,
        callee: Arc<function::Instantiation>,
        arguments: A,
        result: &mut R,
    ) -> ValidationResult<()> {
        let signature = callee.signature();
        let parameter_types = signature.parameter_types();
        let mut actual_arguments = Vec::with_capacity(parameter_types.len());
        for (index, (expected_argument_type, argument)) in parameter_types.iter().zip(arguments.into_iter()).enumerate() {
            let actual_type = argument.value_type();
            if actual_type != expected_argument_type {
                fail!(ValidationError::ArgumentTypeMismatch {
                    expected: expected_argument_type.clone(),
                    actual: actual_type.clone(),
                    index,
                })
            }

            actual_arguments.push(argument);
        }

        if actual_arguments.len() != parameter_types.len() {
            fail!(ValidationError::ArgumentCountMismatch {
                expected: parameter_types.len(),
                actual: actual_arguments.len(),
            });
        }

        for return_type in callee.signature().result_types().iter() {
            result.append_result(self.define_temporary(return_type.clone()))
        }

        Ok(())
    }
}

pub struct BuilderCache {
    instruction_buffer: Vec<Instruction>,
    value_buffer: Vec<instruction_set::Value>,
    input_buffer: Vec<Input>,
    temporary_buffer: elsa::vec::FrozenVec<Box<Temporary>>,
}

impl BuilderCache {
    pub fn new() -> Self {
        Self {
            instruction_buffer: Vec::with_capacity(32),
            value_buffer: Vec::default(),
            input_buffer: Vec::default(),
            temporary_buffer: elsa::vec::FrozenVec::default(),
        }
    }

    #[must_use]
    pub fn builder<R: Into<Box<[type_system::Any]>>, I: IntoIterator<Item = type_system::Any>>(
        &mut self,
        result_types: R,
        input_types: I,
    ) -> Builder<'_> {
        self.instruction_buffer.clear();
        self.value_buffer.clear();
        self.input_buffer.clear();
        self.temporary_buffer.as_mut().clear();

        for (index, input_type) in input_types.into_iter().enumerate() {
            self.input_buffer.push(Input {
                index,
                value_type: input_type,
            });
        }

        let result_types = result_types.into();
        let mut integer_size = VarIntSize::One;
        integer_size.resize_to_fit(self.input_buffer.len());
        integer_size.resize_to_fit(result_types.len());

        Builder {
            integer_size,
            result_types,
            input_registers: &self.input_buffer,
            temporary_registers: &self.temporary_buffer,
            instructions: &mut self.instruction_buffer,
            value_buffer: &mut self.value_buffer,
        }
    }
}

impl std::default::Default for BuilderCache {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    integer_size: VarIntSize,
    input_types: Box<[type_system::Any]>,
    result_types: Box<[type_system::Any]>,
    temporary_types: Box<[type_system::Any]>,
    instructions: Box<[Instruction]>,
}

impl Block {
    #[inline]
    pub(crate) fn integer_size(&self) -> VarIntSize {
        self.integer_size
    }

    #[inline]
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    #[inline]
    pub fn input_types(&self) -> &[type_system::Any] {
        &self.input_types
    }

    #[inline]
    pub fn result_types(&self) -> &[type_system::Any] {
        &self.result_types
    }

    #[inline]
    pub fn temporary_types(&self) -> &[type_system::Any] {
        &self.temporary_types
    }
}

impl std::cmp::PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.input_types() == other.input_types()
            && self.result_types() == other.result_types()
            && self.temporary_types() == other.temporary_types()
            && self.instructions() == other.instructions()
    }
}

impl std::cmp::Eq for Block {}

impl Hash for Block {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self.input_types(), state);
        Hash::hash(self.result_types(), state);
        Hash::hash(self.temporary_types(), state);
        Hash::hash(self.instructions(), state);
    }
}
