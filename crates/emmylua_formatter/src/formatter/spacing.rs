use emmylua_parser::BinaryOperator;

use crate::config::LuaFormatConfig;
use crate::ir::{self, DocIR};

/// Spacing decision for a token boundary.
///
/// This centralizes all "should there be a space here?" logic into a single
/// declarative system, decoupled from the recursive IR-building code.
///
/// Format functions query this system instead of hard-coding `ir::space()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SpaceRule {
    /// Must have exactly one space
    Space,
    /// Must have no space
    NoSpace,
    /// Soft line break — becomes space in flat mode, newline in break mode.
    /// Use for positions that may line-wrap.
    SoftLine,
    /// Soft line break or empty — becomes empty in flat mode, newline in break mode
    SoftLineOrEmpty,
}

impl SpaceRule {
    /// Convert a SpaceRule into the corresponding DocIR node
    pub fn to_ir(self) -> DocIR {
        match self {
            SpaceRule::Space => ir::space(),
            SpaceRule::NoSpace => ir::list(vec![]),
            SpaceRule::SoftLine => ir::soft_line(),
            SpaceRule::SoftLineOrEmpty => ir::soft_line_or_empty(),
        }
    }
}

/// Resolve spacing around a binary operator.
///
/// Controls whether spaces appear around `+`, `-`, `*`, `/`, `and`, `..`, etc.
pub fn space_around_binary_op(op: BinaryOperator, config: &LuaFormatConfig) -> SpaceRule {
    match op {
        // Arithmetic: + - * / // % ^
        BinaryOperator::OpAdd
        | BinaryOperator::OpSub
        | BinaryOperator::OpMul
        | BinaryOperator::OpDiv
        | BinaryOperator::OpIDiv
        | BinaryOperator::OpMod
        | BinaryOperator::OpPow => {
            if config.spacing.space_around_math_operator {
                SpaceRule::Space
            } else {
                SpaceRule::NoSpace
            }
        }

        // Comparison: == ~= < > <= >=
        BinaryOperator::OpEq
        | BinaryOperator::OpNe
        | BinaryOperator::OpLt
        | BinaryOperator::OpGt
        | BinaryOperator::OpLe
        | BinaryOperator::OpGe => SpaceRule::Space,

        // Logical: and or — always spaces (keyword operators)
        BinaryOperator::OpAnd | BinaryOperator::OpOr => SpaceRule::Space,

        // Concatenation: ..
        BinaryOperator::OpConcat => {
            if config.spacing.space_around_concat_operator {
                SpaceRule::Space
            } else {
                SpaceRule::NoSpace
            }
        }

        // Bitwise: & | ~ << >>
        BinaryOperator::OpBAnd
        | BinaryOperator::OpBOr
        | BinaryOperator::OpBXor
        | BinaryOperator::OpShl
        | BinaryOperator::OpShr => SpaceRule::Space,

        BinaryOperator::OpNop => SpaceRule::Space,
    }
}

/// Resolve spacing around the assignment `=` operator.
pub fn space_around_assign(config: &LuaFormatConfig) -> SpaceRule {
    if config.spacing.space_around_assign_operator {
        SpaceRule::Space
    } else {
        SpaceRule::NoSpace
    }
}
