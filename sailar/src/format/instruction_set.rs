use super::{indices, type_system, LenVec};
use crate::hashing::IntegerHashBuilder;
use bitflags::bitflags;
use std::collections::hash_map;

pub use indices::{
    CodeBlock as BlockIndex, Field as FieldIndex, Function as FunctionIndex,
    Register as RegisterIndex, TypeSignature as TypeIndex,
};

/// Represents an integer constant, whose value is stored in little-endian order.
#[derive(Clone, Copy, Debug, Eq)]
pub enum IntegerConstant {
    U8(u8),
    S8(i8),
    U16(u16),
    S16(i16),
    U32(u32),
    S32(i32),
    U64(u64),
    S64(i64),
}

impl IntegerConstant {
    pub fn value_type(self) -> type_system::FixedInt {
        use type_system::FixedInt;
        match self {
            Self::U8(_) => FixedInt::U8,
            Self::S8(_) => FixedInt::S8,
            Self::U16(_) => FixedInt::U16,
            Self::S16(_) => FixedInt::S16,
            Self::U32(_) => FixedInt::U32,
            Self::S32(_) => FixedInt::S32,
            Self::U64(_) => FixedInt::U64,
            Self::S64(_) => FixedInt::S64,
        }
    }

    pub fn value(self) -> i128 {
        match self {
            Self::U8(value) => value.into(),
            Self::S8(value) => value.into(),
            Self::U16(value) => value.into(),
            Self::S16(value) => value.into(),
            Self::U32(value) => value.into(),
            Self::S32(value) => value.into(),
            Self::U64(value) => value.into(),
            Self::S64(value) => value.into(),
        }
    }
}

macro_rules! integer_constant_conversion_from_integer {
    ($integer_type: ty, $case_name: ident) => {
        impl From<$integer_type> for IntegerConstant {
            fn from(value: $integer_type) -> Self {
                Self::$case_name(value)
            }
        }
    };
}

integer_constant_conversion_from_integer!(u8, U8);
integer_constant_conversion_from_integer!(i8, S8);
integer_constant_conversion_from_integer!(u16, U16);
integer_constant_conversion_from_integer!(i16, S16);
integer_constant_conversion_from_integer!(u32, U32);
integer_constant_conversion_from_integer!(i32, S32);
integer_constant_conversion_from_integer!(u64, U64);
integer_constant_conversion_from_integer!(i64, S64);

impl std::cmp::PartialEq for IntegerConstant {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl std::hash::Hash for IntegerConstant {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        state.write_i128(self.value())
    }
}

/*
pub enum Value {
    Register(RegisterIndex),
    Integer(IntegerConstant)
}
*/

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
#[repr(u32)]
pub enum Opcode {
    Nop = 0,
    Ret,
    #[deprecated = "Reserved, phi instructions are not expected to be used in the future since branching with block inputs is used instead."]
    Phi,
    Select,
    Switch,
    Br,
    BrIf,
    Call,
    CallIndr,
    CallRet,
    Add,
    Sub,
    Mul,
    Div,
    And,
    Or,
    Not,
    Xor,
    Rem,
    Mod,
    DivRem,
    ShL,
    ShR,
    RotL,
    RotR,
    ConstI,
    ConstF,
    Cmp,
    PopCnt,
    Clz,
    Ctz,
    Reverse,
    Function,
    ConvI,
    ConvF,
    Field,
    Global,
    MemSt,
    MemLd,
    MemCpy,
    Alloca = 253,
    Break = 254,
    /// Not an instruction, indicates that there are more opcode bytes to follow.
    Continuation = 0xFF,
}

bitflags! {
    #[repr(transparent)]
    pub struct ArithmeticFlags: u8 {
        const NONE = 0;
        const FLAG_ON_OVERFLOW = 0b0000_0001;
        const RETURN_VALUE_ON_DIVIDE_BY_ZERO = 0b0000_0100;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OverflowBehavior {
    Ignore = 0,
    /// Introduces an extra temporary register containing a boolean value indicating if an overflow occured.
    Flag = 1,
}

// Could remove, only needed for ConvI
impl TryFrom<u8> for OverflowBehavior {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Ignore),
            1 => Ok(Self::Flag),
            _ => Err(value),
        }
    }
}

impl From<OverflowBehavior> for ArithmeticFlags {
    fn from(behavior: OverflowBehavior) -> Self {
        match behavior {
            OverflowBehavior::Ignore => Self::NONE,
            OverflowBehavior::Flag => Self::FLAG_ON_OVERFLOW,
        }
    }
}

impl From<ArithmeticFlags> for OverflowBehavior {
    fn from(flags: ArithmeticFlags) -> Self {
        if flags.contains(ArithmeticFlags::FLAG_ON_OVERFLOW) {
            Self::Flag
        } else {
            Self::Ignore
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DivideByZeroBehavior {
    /// Indicates that the value contained in the specified register should be returned if a division by zero occured.
    Return(RegisterIndex),
    Halt,
}

impl DivideByZeroBehavior {
    pub fn flags(self) -> ArithmeticFlags {
        match self {
            Self::Return(_) => ArithmeticFlags::RETURN_VALUE_ON_DIVIDE_BY_ZERO,
            Self::Halt => ArithmeticFlags::NONE,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct BasicArithmeticOperation {
    pub overflow: OverflowBehavior,
    pub x: RegisterIndex,
    pub y: RegisterIndex,
}

impl BasicArithmeticOperation {
    pub fn flags(&self) -> ArithmeticFlags {
        ArithmeticFlags::from(self.overflow)
    }
}

/// # Structure
/// - [`function`]
/// - [`arguments`]
#[derive(Debug, PartialEq)]
pub struct CallInstruction {
    pub function: FunctionIndex,
    pub arguments: LenVec<RegisterIndex>,
}

/// Specifies the targets of a `switch` instruction.
///
/// # Structure
/// - [`len()`]
/// - value0
/// - target0
/// - value1
/// - target1
/// - ...
#[derive(Debug, PartialEq)]
pub struct SwitchLookupTable {
    lookup: hash_map::HashMap<IntegerConstant, BlockIndex, IntegerHashBuilder>,
}

impl SwitchLookupTable {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            lookup: hash_map::HashMap::with_capacity_and_hasher(
                capacity,
                IntegerHashBuilder::default(),
            ),
        }
    }

    #[must_use]
    pub fn insert(&mut self, value: IntegerConstant, target: BlockIndex) -> bool {
        match self.lookup.entry(value) {
            hash_map::Entry::Vacant(vacant) => {
                vacant.insert(target);
                true
            }
            hash_map::Entry::Occupied(_) => false,
        }
    }

    pub fn get(&self, value: &IntegerConstant) -> Option<BlockIndex> {
        self.lookup.get(value).copied()
    }

    pub fn len(&self) -> usize {
        self.lookup.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lookup.is_empty()
    }

    pub fn iter(&self) -> impl std::iter::Iterator<Item = (&'_ IntegerConstant, BlockIndex)> + '_ {
        self.lookup.iter().map(|(value, target)| (value, *target))
    }
}

/// Indicates the kind of comparison performed on the operands of a `cmp` instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ComparisonKind {
    Equal = 0,
    NotEqual,
    /// Checks if `x` is less than `y`.
    LessThan,
    /// Checks if `x` is greater than `y`.
    GreaterThan,
    /// Checks if `x` is less than or equal to `y`.
    LessThanOrEqual,
    /// Checks if `x` is greater than or equal to `y`.
    GreaterThanOrEqual,
}

impl TryFrom<u8> for ComparisonKind {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value <= Self::GreaterThanOrEqual as u8 {
            Ok(unsafe { std::mem::transmute(value) })
        } else {
            Err(value)
        }
    }
}

/// Represents an instruction consisting of an opcode and one or more operands.
#[derive(Debug, PartialEq)]
pub enum Instruction {
    // TODO: Should case fields be Boxed?
    /// ```txt
    /// nop;
    /// ```
    /// Does absolutely nothing.
    Nop,
    /// ```txt
    /// ret <value0>, <value1>, ...;
    /// ```
    /// Returns the values in the specified registers and transfers control back to the calling function.
    ///
    /// # Requirements
    /// - Should be the last instruction in a block.
    Ret(LenVec<RegisterIndex>),
    ///// ```txt
    ///// <result0>, <result1>, ... = select <condition> then <value0>, <value1>, ... else <value2>, <value3>, ...;
    ///// ```
    ///// Used to select one set of values or the other depending on whether the value in the `condition` register is truthy,
    ///// without requires blocks or branching.
    //Select {
    //    condition: RegisterIndex,
    //    true_registers: LenVec<RegisterIndex>, // NOTE: Length of vectors should be the same anyway, could optimize away length.
    //    false_registers: Vec<RegisterIndex>,
    //},
    /// ```txt
    /// switch <cmpty> <comparison> default <target0> or <value1> <target1> or <value2> <target2> or ...;
    /// ```
    /// Transfers control to one of several blocks depending on the value in the `comparison` register.
    ///
    /// # Requirements
    /// - Should be the last instruction in a block.
    /// - The type of the value stored in the `comparison` register (the `cmpty`), __must__ be a fixed-size integer
    /// type.
    Switch {
        comparison: RegisterIndex,
        comparison_type: type_system::FixedInt,
        default_target: BlockIndex,
        target_lookup: SwitchLookupTable,
    },
    /// ```txt
    /// br <target>;
    /// br <target> with <input0>, <input1>, ...;
    /// ```
    /// Unconditionally transfers control flow to the `target` block providing the specified `input` values.
    ///
    /// # Requirements
    /// - Should be the last instruction in a block.
    Br {
        target: BlockIndex,
        input_registers: LenVec<RegisterIndex>,
    },
    /// ```txt
    /// br.if <condition> then <true> else <false>;
    /// br.if <condition> then <true> else <false> with <input0>, <input1>, ...;
    /// ```
    /// If the value in the `condition` register is truthy (not equal to zero), transfers control flow to the `true` block;
    /// otherwise, control flow is transferred to the `false` block.
    ///
    /// # Requirements
    /// - Should be the last instruction in a block.
    BrIf {
        condition: RegisterIndex,
        true_branch: BlockIndex,
        false_branch: BlockIndex,
        input_registers: LenVec<RegisterIndex>,
    },
    /// ```txt
    /// <result0>, <result1>, ... = call <function> <argument0>, <argument1>, ...;
    /// ```
    /// Calls the specified `function`, supplying the values in the arguments registers as inputs to its entry block. The number
    /// of temporary registers introduced is equal to the number of return values in the function's signature.
    ///
    /// # Requirements
    /// - The number of registers used as arguments __must__ exactly match the number of arguments specified by the signature of
    /// the function.
    Call(CallInstruction),
    //CallIndr
    //CallRet
    /// ```txt
    /// <sum> = add <x> to <y>;
    /// <sum>, <overflowed> = add <x> to <y> ovf.flag;
    /// ```
    /// Returns the sum of the values in the `x` and `y` registers.
    Add(BasicArithmeticOperation),
    /// ```txt
    /// <result> = sub <x> from <y>;
    /// <result>, <overflowed> = sub <x> from <y> ovf.flag;
    /// ```
    /// Subtracts the value in the `x` register from the value in the `y` register, and returns
    /// the difference.
    Sub(BasicArithmeticOperation),
    /// ```txt
    /// <product> = mul <x> by <y>;
    /// <product>, <overflowed> = mul <x> by <y> ovf.flag;
    /// ```
    /// Returns the product of the values in the `x` and `y` registers.
    Mul(BasicArithmeticOperation),
    /// ```txt
    /// <result> = const.i <ity> <value>;
    /// ```
    /// Returns an integer of the specified type.
    ///
    /// # Structure
    /// - [`Opcode`]
    /// - [`IntegerConstant::value_type()`]
    /// - [`IntegerConstant::value()`]
    ConstI(IntegerConstant), // TODO: Allow indicating if integer constant is of a pointer type?
    /// ```txt
    /// <result> = conv.i <operand> to <ity>;
    /// <result>, <overflowed> = conv.i <operand> to <ity> ovf.flag;
    /// ```
    /// Converts the value stored in the `operand` register to an integer of the specified type.
    /// - For conversion from a signed integer to a larger signed integer, performs sign extension.
    /// - For conversion from an unsigned integer to a larger unsigned integer, performs zero extension.
    /// - Truncates when converting from one integer type to a smaller integer type.
    ConvI {
        target_type: type_system::Int,
        overflow: OverflowBehavior,
        operand: RegisterIndex,
    },
    ///// ```txt
    ///// <result> = conv.f <operand> to <fty>;
    ///// ```
    ///// Converts the value stored in the `operand` register to a floating-point number of the specified type.
    ///// - Extends or truncates as necessary when converting from one floating-point type to another.
    ///// - For integer to floating-point type conversions, rounds (to what?)
    //ConvF {
    //    target_type: type_system: Real,
    //    operand: RegisterIndex
    //},
    /// ```txt
    /// <result> = cmp <x> eq <y>;
    /// <result> = cmp <x> ne <y>;
    /// <result> = cmp <x> lt <y>;
    /// <result> = cmp <x> gt <y>;
    /// <result> = cmp <x> le <y>;
    /// <result> = cmp <x> ge <y>;
    /// ```
    /// Performs a comparison, returning an unsigned byte `1` if the comparison performed is true, and `0` otherwise.
    Cmp {
        x: RegisterIndex,
        kind: ComparisonKind,
        y: RegisterIndex,
    },
    /// ```txt
    /// <address> = field <field> of <ty> in <object>;
    /// ```
    /// Returns a pointer to the `field` in the specified `object`.
    ///
    /// # Requirements
    /// - The value in the `object` register must be of a pointer type to the struct type `ty`.
    Field {
        field: FieldIndex,
        object: RegisterIndex,
    },
    // TODO: Have flag to avoid zero-ing memory.
    /// ```txt
    /// <result> = alloca <amount> of <type>;
    /// ```
    /// Returns a suitably aligned pointer to newly allocated memory on the stack to contain `amount` elements of the specified
    /// `type`. If the allocation fails, a `null` pointer is returned. The memory allocated is automatically freed when the
    /// function returns.
    Alloca {
        amount: RegisterIndex,
        element_type: TypeIndex,
    },
    /// ```txt
    /// break;
    /// ```
    /// Represents a breakpoint placed by a debugger.
    ///
    /// If no debugger is attached, or if a debugger is not supported, this instruction does nothing.
    Break,
}

impl Instruction {
    pub fn opcode(&self) -> Opcode {
        match self {
            Instruction::Nop => Opcode::Nop,
            Instruction::Ret(_) => Opcode::Ret,
            Instruction::Switch { .. } => Opcode::Switch,
            Instruction::Br { .. } => Opcode::Br,
            Instruction::BrIf { .. } => Opcode::BrIf,
            Instruction::Call(_) => Opcode::Call,
            Instruction::Add(_) => Opcode::Add,
            Instruction::Sub(_) => Opcode::Sub,
            Instruction::Mul(_) => Opcode::Mul,
            // Instruction::Div(_) => Opcode::Div,
            // Instruction::And(_) => Opcode::And,
            // Instruction::Or(_) => Opcode::Or,
            // Instruction::Not { .. } => Opcode::Not,
            // Instruction::Xor(_) => Opcode::Xor,
            // Instruction::ShL(_) => Opcode::ShL,
            // Instruction::ShR(_) => Opcode::ShR,
            // Instruction::RotL(_) => Opcode::RotL,
            // Instruction::RotR(_) => Opcode::RotR,
            Instruction::ConstI(_) => Opcode::ConstI,
            Instruction::ConvI { .. } => Opcode::ConvI,
            Instruction::Cmp { .. } => Opcode::Cmp,
            Instruction::Field { .. } => Opcode::Field,
            Instruction::Alloca { .. } => Opcode::Alloca,
            Instruction::Break => Opcode::Break,
        }
    }

    /// Calculates the number of temporary registers introduced after execution of the instruction.
    pub fn return_count<R: FnOnce(FunctionIndex) -> usize>(
        &self,
        function_return_count: R,
    ) -> usize {
        match self {
            Instruction::Nop
            | Instruction::Ret(_)
            | Instruction::Switch { .. }
            | Instruction::Br { .. }
            | Instruction::BrIf { .. }
            | Instruction::Break => 0,
            Instruction::Call(CallInstruction { function, .. }) => function_return_count(*function),
            Instruction::Add(BasicArithmeticOperation { overflow, .. })
            | Instruction::Sub(BasicArithmeticOperation { overflow, .. })
            | Instruction::Mul(BasicArithmeticOperation { overflow, .. })
            //| Instruction::Div(DivisionOperation { overflow, .. })
            | Instruction::ConvI { overflow, .. }
            => match overflow {
                OverflowBehavior::Ignore => 1,
                OverflowBehavior::Flag => 2,
            },
            // Instruction::And(_)
            // | Instruction::Or(_)
            // | Instruction::Not { .. }
            // | Instruction::Xor(_)
            // | Instruction::ShL(_)
            // | Instruction::ShR(_)
            // | Instruction::RotL(_)
            // | Instruction::RotR(_)
            | Instruction::ConstI(_)
            | Instruction::Cmp { .. }
            | Instruction::Field { .. }
            | Instruction::Alloca { .. } => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
#[error("{value:#04X} is not a valid opcode")]
pub struct InvalidOpcodeError {
    pub value: u32,
}

impl TryFrom<u32> for Opcode {
    type Error = InvalidOpcodeError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        #[allow(deprecated)]
        if value != Self::Phi as u32
            && (value <= Self::Field as u32
                || value == Self::Alloca as u32
                || value == Self::Break as u32)
        {
            Ok(unsafe { std::mem::transmute(value) })
        } else {
            Err(InvalidOpcodeError { value })
        }
    }
}