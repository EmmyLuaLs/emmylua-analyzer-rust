//! # Bytecode-style type inference VM
//!
//! Compile: expression tree -> linear bytecode (task-stack traversal, zero recursion).
//! Interpret: flat PC loop + value stack + `computing` cycle protection (LuaType has no
//! fixpoint domain, so cycle re-entry pushes Unknown).
//!
//! Values = `LuaType` + optional owner (for member lookup). Cross-file member/declaration
//! resolution is delegated to salsa queries.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaDocTag, LuaDocType, LuaExpr,
    LuaIndexKey, LuaReturnStat, LuaSyntaxId,
};

use smol_str::SmolStr;

use crate::salsa_builder::def::{
    ConstructorAttribute, ConstructorReturnMode, DeclKind, SalsaGenericParam, SemanticId,
    Signature, SignatureDoc,
};
use crate::salsa_builder::types::PrimitiveType;
use crate::{
    AsyncState, FileId, GenericTpl, GenericTplId, InFiled, LuaAliasCallKind, LuaArrayType,
    LuaFunctionType, LuaGenericType, LuaMemberKey, LuaObjectType, LuaTupleStatus, LuaTupleType,
    LuaType, LuaTypeDeclId, LuaTypeNode, LuaUnionType, TypeDef, TypeDefKind, TypeScope,
    TypeVisibility, VariadicType, WorkspaceId, query,
};

use super::super::SemanticModel;
use super::unify;

#[derive(Debug, Clone)]
pub enum Instr {
    /// Literal primitive.
    PushPrimitive(PrimitiveType),
    /// Concrete value type (string constants / anonymous table identity etc.).
    PushValue(LuaType),
    PushUnknown,
    /// Closure reference (marks syntax for Call to back-infer params).
    PushClosure {
        syntax: LuaSyntaxId,
    },
    /// Name reference -> (owner, type).
    LoadName {
        name: String,
        offset: rowan::TextSize,
    },
    /// Pop owner value -> member access -> push (member_type, member_id).
    IndexMember {
        /// Static member key (`Name` / `Integer`); `None` means a dynamic key (`t[k]`).
        key: Option<LuaMemberKey>,
        dynamic: bool,
    },
    /// Pop callee + n args -> call inference (unify and substitute return).
    Call {
        arg_count: usize,
        colon_call: bool,
        generic_type_syntaxes: Vec<LuaSyntaxId>,
        arg_syntaxes: Vec<LuaSyntaxId>,
    },
    /// Unary operation (unm/len, including operator overload): pop operand -> result.
    UnaryOp {
        op: emmylua_parser::UnaryOperator,
    },
    /// Binary operation: pop two -> result.
    Binary {
        op: BinaryOperator,
        /// Left expression syntax (short-circuit evaluation needs nullable-member awareness).
        left_syntax: Option<LuaSyntaxId>,
        /// Right expression syntax (kept for future extension).
        right_syntax: Option<LuaSyntaxId>,
    },
    /// Top of stack is the result.
    Result,
}

struct ClosureReturnInferGuard<'a> {
    model: &'a SemanticModel<'a>,
    closure_syntax: LuaSyntaxId,
}

impl<'a> ClosureReturnInferGuard<'a> {
    fn new(model: &'a SemanticModel<'a>, closure_syntax: LuaSyntaxId) -> Self {
        model.begin_closure_return_infer(closure_syntax);
        Self {
            model,
            closure_syntax,
        }
    }
}

impl Drop for ClosureReturnInferGuard<'_> {
    fn drop(&mut self) {
        self.model.end_closure_return_infer(self.closure_syntax);
    }
}

// ──────────────────────────────────────────────
// Values
// ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Value {
    pub ty: LuaType,
    pub owner: Option<SemanticId>,
    pub closure_syntax: Option<LuaSyntaxId>,
    /// Receiver type before member access (used for method `self` substitution).
    pub receiver: Option<LuaType>,
}

impl Value {
    fn plain(ty: LuaType) -> Self {
        Self {
            ty,
            owner: None,
            closure_syntax: None,
            receiver: None,
        }
    }
}

enum CTask {
    Expr(LuaExpr),
    Index {
        key: Option<LuaMemberKey>,
    },
    EmitIndex {
        key: Option<LuaMemberKey>,
    },
    Call {
        arg_count: usize,
        colon_call: bool,
        generic_type_syntaxes: Vec<LuaSyntaxId>,
        arg_syntaxes: Vec<LuaSyntaxId>,
    },
    /// Unary operation (unm/len): compile the operand first, then emit a UnaryOp instruction.
    UnaryOp {
        op: emmylua_parser::UnaryOperator,
    },
    /// Binary operation: compile `right` first, then emit a Binary instruction.
    BinaryR {
        op: BinaryOperator,
        right: LuaExpr,
        left_syntax: Option<LuaSyntaxId>,
    },
    EmitBinary {
        op: BinaryOperator,
        left_syntax: Option<LuaSyntaxId>,
        right_syntax: Option<LuaSyntaxId>,
    },
}

/// Compile an expression -> linear bytecode.
pub fn compile(expr: LuaExpr, file_id: FileId, out: &mut Vec<Instr>) {
    let mut stack = vec![CTask::Expr(expr)];
    while let Some(task) = stack.pop() {
        match task {
            CTask::Expr(expr) => compile_expr(&expr, file_id, out, &mut stack),
            CTask::Index { key } => out.push(Instr::IndexMember {
                key,
                dynamic: false,
            }),
            CTask::EmitIndex { key } => out.push(Instr::IndexMember { key, dynamic: true }),
            CTask::Call {
                arg_count,
                colon_call,
                generic_type_syntaxes,
                arg_syntaxes,
            } => out.push(Instr::Call {
                arg_count,
                colon_call,
                generic_type_syntaxes,
                arg_syntaxes,
            }),
            CTask::UnaryOp { op } => out.push(Instr::UnaryOp { op }),
            CTask::BinaryR {
                op,
                right,
                left_syntax,
            } => {
                // Order: compile `right` first (left is already at the bottom of the value
                // stack), then emit Binary.
                stack.push(CTask::EmitBinary {
                    op,
                    left_syntax,
                    right_syntax: Some(right.get_syntax_id()),
                });
                stack.push(CTask::Expr(right));
            }
            CTask::EmitBinary {
                op,
                left_syntax,
                right_syntax,
            } => out.push(Instr::Binary {
                op,
                left_syntax,
                right_syntax,
            }),
        }
    }
}

fn index_key_to_member_key(key: &LuaIndexKey) -> Option<LuaMemberKey> {
    match key {
        LuaIndexKey::Name(name) => Some(LuaMemberKey::Name(SmolStr::new(name.get_name_text()))),
        LuaIndexKey::String(s) => Some(LuaMemberKey::Name(SmolStr::new(s.get_value()))),
        LuaIndexKey::Integer(i) => match i.get_number_value() {
            emmylua_parser::NumberResult::Int(n) => Some(LuaMemberKey::Integer(n)),
            emmylua_parser::NumberResult::Uint(n) => Some(LuaMemberKey::Integer(n as i64)),
            _ => None,
        },
        LuaIndexKey::Idx(i) => Some(LuaMemberKey::Integer(*i as i64)),
        LuaIndexKey::Expr(_) => None,
    }
}

fn compile_expr(expr: &LuaExpr, file_id: FileId, out: &mut Vec<Instr>, stack: &mut Vec<CTask>) {
    match expr {
        LuaExpr::LiteralExpr(literal) => {
            match literal.get_literal() {
                // String literal -> StringConst (require module names / member keys consume
                // constant info).
                Some(emmylua_parser::LuaLiteralToken::String(str)) => {
                    out.push(Instr::PushValue(LuaType::StringConst(
                        SmolStr::new(str.get_value()).into(),
                    )));
                }
                Some(emmylua_parser::LuaLiteralToken::Number(number)) => {
                    match number.get_number_value() {
                        emmylua_parser::NumberResult::Int(i) => {
                            out.push(Instr::PushValue(LuaType::IntegerConst(i)));
                        }
                        emmylua_parser::NumberResult::Uint(u) => {
                            out.push(Instr::PushValue(LuaType::IntegerConst(u as i64)));
                        }
                        emmylua_parser::NumberResult::Float(f) => {
                            out.push(Instr::PushValue(LuaType::FloatConst(f)));
                        }
                        emmylua_parser::NumberResult::Number => {
                            out.push(Instr::PushPrimitive(PrimitiveType::Number));
                        }
                    }
                }
                Some(emmylua_parser::LuaLiteralToken::Bool(bool)) => {
                    // Boolean literal -> BooleanConst (is_always_truthy/falsy predicates
                    // consume const info).
                    out.push(Instr::PushValue(LuaType::BooleanConst(bool.is_true())));
                }
                Some(emmylua_parser::LuaLiteralToken::Nil(_)) => {
                    out.push(Instr::PushPrimitive(PrimitiveType::Nil));
                }
                Some(emmylua_parser::LuaLiteralToken::Dots(_)) => {
                    // Variadic args `...`: load by name and let `load_name` resolve the
                    // enclosing closure's variadic type.
                    out.push(Instr::LoadName {
                        name: "...".to_string(),
                        offset: literal.syntax().text_range().start(),
                    });
                }
                _ => {
                    out.push(Instr::PushUnknown);
                }
            };
        }
        LuaExpr::NameExpr(name_expr) => {
            if let Some(name) = name_expr.get_name_text() {
                out.push(Instr::LoadName {
                    name: name.to_string(),
                    offset: name_expr.get_position(),
                });
            } else {
                out.push(Instr::PushUnknown);
            }
        }
        LuaExpr::IndexExpr(index_expr) => {
            let index_key = index_expr.get_index_key();
            let key = index_key.as_ref().and_then(index_key_to_member_key);
            if let Some(prefix) = index_expr.get_prefix_expr() {
                if let Some(LuaIndexKey::Expr(key_expr)) = index_key {
                    // Dynamic key: prefix, key, IndexMember(dynamic).
                    stack.push(CTask::EmitIndex { key });
                    stack.push(CTask::Expr(key_expr));
                    stack.push(CTask::Expr(prefix));
                } else {
                    stack.push(CTask::Index { key });
                    stack.push(CTask::Expr(prefix));
                }
            } else {
                out.push(Instr::PushUnknown);
            }
        }
        LuaExpr::CallExpr(call_expr) => {
            let arg_count = call_expr.get_args_count().unwrap_or(0);
            let mut args = Vec::new();
            if let Some(arg_list) = call_expr.get_args_list() {
                args = arg_list.get_args().collect();
            }
            if let Some(prefix) = call_expr.get_prefix_expr() {
                let generic_type_syntaxes = call_expr
                    .get_call_generic_type_list()
                    .map(|list| {
                        list.get_types()
                            .map(|ty| ty.get_syntax_id())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let arg_syntaxes: Vec<LuaSyntaxId> = call_expr
                    .get_args_list()
                    .map(|list| list.get_args().map(|arg| arg.get_syntax_id()).collect())
                    .unwrap_or_default();
                stack.push(CTask::Call {
                    arg_count,
                    colon_call: call_expr.is_colon_call(),
                    generic_type_syntaxes,
                    arg_syntaxes,
                });
                for arg in args.into_iter().rev() {
                    stack.push(CTask::Expr(arg));
                }
                stack.push(CTask::Expr(prefix));
            } else {
                out.push(Instr::PushUnknown);
            }
        }
        LuaExpr::BinaryExpr(binary) => {
            if let (Some(op), Some((left, right))) = (
                binary.get_op_token().map(|t| t.get_op()),
                binary.get_exprs(),
            ) {
                stack.push(CTask::BinaryR {
                    op,
                    right,
                    left_syntax: Some(left.get_syntax_id()),
                });
                stack.push(CTask::Expr(left));
            } else {
                out.push(Instr::PushUnknown);
            }
        }
        LuaExpr::ClosureExpr(closure) => {
            out.push(Instr::PushClosure {
                syntax: closure.get_syntax_id(),
            });
        }
        LuaExpr::ParenExpr(paren) => {
            if let Some(inner) = paren.get_expr() {
                stack.push(CTask::Expr(inner));
            } else {
                out.push(Instr::PushUnknown);
            }
        }
        // Anonymous table literal: TableConst preserves synthetic identity
        // ((file, range) -> member lookup).
        LuaExpr::TableExpr(table) => out.push(Instr::PushValue(LuaType::TableConst(InFiled::new(
            file_id,
            table.get_range(),
        )))),
        LuaExpr::UnaryExpr(unary) => {
            // M0: `not` -> Boolean; `-x`/`#x` -> operator overload (`---@operator unm/len`),
            // falling back to the operand type when no overload exists.
            let op = unary.get_op_token().map(|t| t.get_op());
            if op == Some(emmylua_parser::UnaryOperator::OpNot) {
                out.push(Instr::PushPrimitive(PrimitiveType::Boolean));
            } else if let Some(inner) = unary.get_expr() {
                stack.push(CTask::UnaryOp {
                    op: op.unwrap_or(emmylua_parser::UnaryOperator::OpUnm),
                });
                stack.push(CTask::Expr(inner));
            } else {
                out.push(Instr::PushUnknown);
            }
        }
        _ => out.push(Instr::PushUnknown),
    }
}

// ──────────────────────────────────────────────
// Interpreter (flat PC loop)
// ──────────────────────────────────────────────

pub struct InferVm<'a> {
    model: &'a SemanticModel<'a>,
    code: &'a [Instr],
    pc: usize,
    stack: Vec<Value>,
    /// Cycle guard: declarations/members currently being computed.
    computing: HashSet<SemanticId>,
    /// Closure parameter environment (filled by Call, read by LoadName for Params).
    pub closure_params: HashMap<(LuaSyntaxId, usize), LuaType>,
    /// Recursion depth for callback-call back-inference (prevents infinite expansion of
    /// self-referential calls like `wrap(wrap, ...)`).
    callback_call_depth: usize,
}

impl<'a> InferVm<'a> {
    pub fn new(model: &'a SemanticModel<'a>, code: &'a [Instr]) -> Self {
        Self {
            model,
            code,
            pc: 0,
            stack: Vec::new(),
            computing: HashSet::new(),
            closure_params: HashMap::new(),
            callback_call_depth: 0,
        }
    }

    pub fn run(&mut self) -> LuaType {
        while self.pc < self.code.len() {
            let instr = self.code[self.pc].clone();
            self.pc += 1;
            match instr {
                Instr::Result => break,
                instr => self.execute(instr),
            }
        }
        self.stack
            .last()
            .map(|value| value.ty.clone())
            .unwrap_or(LuaType::Unknown)
    }

    fn execute(&mut self, instr: Instr) {
        match instr {
            Instr::PushPrimitive(p) => {
                let ty = primitive_lua_type(p);
                self.stack.push(Value::plain(ty));
            }
            Instr::PushValue(ty) => self.stack.push(Value::plain(ty)),
            Instr::PushUnknown => self.stack.push(Value::plain(LuaType::Unknown)),
            Instr::PushClosure { syntax } => self.stack.push(Value {
                ty: LuaType::Function,
                owner: None,
                closure_syntax: Some(syntax),
                receiver: None,
            }),
            Instr::LoadName { name, offset } => {
                let value = self.load_name(&name, offset);
                self.stack.push(value);
            }
            Instr::IndexMember { key, dynamic } => {
                if dynamic {
                    let key_value = self.stack.pop().unwrap_or(Value::plain(LuaType::Unknown));
                    let owner_value = self.stack.pop().unwrap_or(Value::plain(LuaType::Unknown));
                    let value = self.index_member_dynamic(owner_value, key_value);
                    self.stack.push(value);
                } else {
                    let owner_value = self.stack.pop().unwrap_or(Value::plain(LuaType::Unknown));
                    let key = key.unwrap_or(LuaMemberKey::Name(SmolStr::new("")));
                    if let LuaMemberKey::Integer(_) = &key
                        && let LuaType::Array(array) = &owner_value.ty
                    {
                        let base = array.get_base().clone();
                        let ty = if self.model.db().strict_array_index() {
                            LuaType::from_vec(vec![base, LuaType::Nil])
                        } else {
                            base
                        };
                        self.stack.push(Value::plain(ty));
                    } else {
                        let value = self.index_member(owner_value, &key);
                        self.stack.push(value);
                    }
                }
            }
            Instr::Call {
                arg_count,
                colon_call,
                generic_type_syntaxes,
                arg_syntaxes,
            } => {
                // Pop args first (collect in reverse -> reverse), then pop callee (pushed first).
                let mut args: Vec<Value> =
                    (0..arg_count).filter_map(|_| self.stack.pop()).collect();
                args.reverse();
                // Inline `--[[@as T]]` and other flow casts are attached to arg expression
                // nodes; use flow-sensitive expression types to override bare VM types so
                // generic calls can back-infer from the cast type.
                for (index, arg) in args.iter_mut().enumerate() {
                    if let Some(syntax) = arg_syntaxes.get(index) {
                        let flow_ty = self
                            .model
                            .type_of_expr_at(*syntax, syntax.get_range().start());
                        if flow_ty != arg.ty && !matches!(flow_ty, LuaType::Unknown | LuaType::Any)
                        {
                            arg.ty = flow_ty;
                        }
                    }
                }
                let callee = self.stack.pop().unwrap_or(Value::plain(LuaType::Unknown));
                let value = self.call(callee, &args, colon_call, &generic_type_syntaxes);
                self.stack.push(value);
            }
            Instr::UnaryOp { op } => {
                let operand = self.stack.pop().unwrap_or(Value::plain(LuaType::Unknown));
                let ty = unary_type(self.model, op, &operand.ty);
                self.stack.push(Value::plain(ty));
            }
            Instr::Binary {
                op,
                left_syntax,
                right_syntax,
            } => {
                let mut right = self.stack.pop().unwrap_or(Value::plain(LuaType::Unknown));
                let mut left = self.stack.pop().unwrap_or(Value::plain(LuaType::Unknown));
                if matches!(op, BinaryOperator::OpAnd | BinaryOperator::OpOr) {
                    if let Some(left_syntax) = left_syntax {
                        if let Some(nullable_extra) =
                            logical_left_nullable_extra(self.model, left_syntax)
                        {
                            left.ty = merge(left.ty.clone(), nullable_extra);
                        }
                    }
                    if let Some(right_syntax) = right_syntax {
                        if let Some(nullable_extra) =
                            logical_left_nullable_extra(self.model, right_syntax)
                        {
                            right.ty = merge(right.ty.clone(), nullable_extra);
                        }
                    }
                }
                let ty = binary_type(self.model, op, &left.ty, &right.ty);
                self.stack.push(Value::plain(ty));
            }
            Instr::Result => {}
        }
    }

    fn load_name(&mut self, name: &str, offset: rowan::TextSize) -> Value {
        // Local declaration.
        if let Some(decl) = self.model.resolve_name(offset) {
            // Closure param: look up the environment.
            if let Some(closure_params) = self.enclosing_closure_params_for_decl(&decl) {
                return Value::plain(closure_params);
            }
            if self.computing.contains(&decl) {
                return Value::plain(LuaType::Unknown);
            }
            self.computing.insert(decl.clone());
            let mut ty = self.model.type_of_decl_at(&decl, offset);
            self.computing.remove(&decl);
            // Lua semantics: an uninitialized local reads as nil in an expression. Use nil
            // only when no declaration/flow type can provide information; do not change
            // iteration variables, `---@type`, module/class-associated declarations, etc.
            if matches!(ty, LuaType::Unknown)
                && let Some(facts) = self.model.file_facts()
                && let Some(info) = facts.decl_by_id(&decl)
                && matches!(info.kind, DeclKind::Local { is_iter: false, .. })
                && info.value_expr_syntax.is_none()
                && info.doc_type_syntax.is_none()
                && info.module_path.is_none()
                && !info.owner_syntax.is_some_and(|owner| {
                    facts
                        .type_defs
                        .iter()
                        .any(|def| def.owner_syntax == Some(owner))
                })
            {
                ty = LuaType::Nil;
            }
            return Value {
                ty,
                owner: Some(decl),
                closure_syntax: None,
                receiver: None,
            };
        }
        // Implicit `self` in method definitions: take the owner type of the member that
        // owns the enclosing closure.
        if name == "self"
            && let Some(value) = self.method_self_value(offset)
        {
            return value;
        }
        // Variadic args `...`: take the enclosing closure's variadic type.
        if name == "..."
            && let Some(ty) = self.enclosing_variadic_type(offset)
        {
            return Value::plain(ty);
        }
        // Global name.
        let owner = SemanticId::name(SmolStr::new(name));
        let ty = self.global_type(&owner);
        Value {
            ty,
            owner: Some(owner),
            closure_syntax: None,
            receiver: None,
        }
    }

    /// Implicit `self` type in method definitions: find the innermost closure containing
    /// `offset`, then use `member.value_syntax` to look up which owner this method belongs
    /// to and take that owner's type.
    fn method_self_value(&self, offset: rowan::TextSize) -> Option<Value> {
        let tree = self.model.syntax_tree()?;
        let chunk = tree.get_chunk_node();
        let closure = chunk
            .descendants::<LuaClosureExpr>()
            .filter(|closure| closure.get_range().contains(offset))
            .min_by_key(|closure| closure.get_range().len())?;
        let closure_syntax = closure.get_syntax_id();
        let facts = self.model.file_facts()?;
        let member = facts
            .members
            .iter()
            .find(|member| member.value_syntax == Some(closure_syntax))?;
        let owner = member.owner.clone();

        let (owner_ty, owner_value) = match &owner {
            SemanticId::Decl(decl) => (
                self.model.type_of_decl(&SemanticId::Decl(decl.clone()))?,
                owner.clone(),
            ),
            SemanticId::TypeDef(def) => {
                let id = match &def.scope {
                    TypeScope::Global => LuaTypeDeclId::global(&def.full_name),
                    TypeScope::Internal(workspace_id) => {
                        LuaTypeDeclId::internal(*workspace_id, &def.full_name)
                    }
                    TypeScope::File(file_id) => LuaTypeDeclId::file(*file_id, &def.full_name),
                };
                let def = self.model.type_def_of(&id)?;
                (self.model.type_def_ref(&def), owner.clone())
            }
            SemanticId::Name(name) => {
                let resolved = self.model.resolve_owner(&SemanticId::Name(name.clone()))?;
                let ty = match &resolved {
                    SemanticId::Decl(decl) => {
                        self.model.type_of_decl(&SemanticId::Decl(decl.clone()))?
                    }
                    SemanticId::TypeDef(def) => {
                        let id = match &def.scope {
                            TypeScope::Global => LuaTypeDeclId::global(&def.full_name),
                            TypeScope::Internal(workspace_id) => {
                                LuaTypeDeclId::internal(*workspace_id, &def.full_name)
                            }
                            TypeScope::File(file_id) => {
                                LuaTypeDeclId::file(*file_id, &def.full_name)
                            }
                        };
                        let def = self.model.type_def_of(&id)?;
                        self.model.type_def_ref(&def)
                    }
                    _ => return None,
                };
                (ty, resolved)
            }
            _ => return None,
        };

        // Consistent with `method_self_return_shell`: if the method owner is a runtime
        // declaration carrying a `---@class/@enum` type definition (same owner_syntax),
        // `self` takes that type definition instead of the bare table literal.
        let (owner_ty, owner_value) = if let SemanticId::Decl(decl_id) = &owner_value {
            let mut resolved = (owner_ty, owner_value.clone());
            if let Some(facts) = self.model.file_facts_of(decl_id.file_id)
                && let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_id.clone()))
                && let Some(def) = facts
                    .type_defs
                    .iter()
                    .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax)
            {
                resolved = (self.model.type_def_ref(def), def.id.clone());
            }
            resolved
        } else {
            (owner_ty, owner_value)
        };

        Some(Value {
            ty: owner_ty,
            owner: Some(owner_value),
            closure_syntax: None,
            receiver: None,
        })
    }

    /// Find the enclosing closure's variadic arg type for `offset` (`...`).
    fn enclosing_variadic_type(&self, offset: rowan::TextSize) -> Option<LuaType> {
        let tree = self.model.syntax_tree()?;
        let chunk = tree.get_chunk_node();
        for closure in chunk.descendants::<LuaClosureExpr>() {
            if !closure.get_range().contains(offset) {
                continue;
            }
            let closure_syntax = closure.get_syntax_id();
            let fun = self.model.type_of_signature(closure_syntax)?;
            for (name, ty) in fun.get_params() {
                if name == "..." {
                    let base = ty.clone().unwrap_or(LuaType::Unknown);
                    return Some(match base {
                        LuaType::Variadic(_) => base,
                        _ => LuaType::Variadic(Arc::new(VariadicType::Base(base))),
                    });
                }
            }
        }
        None
    }

    /// If `decl` is a closure param and that closure is in the environment, infer its type.
    /// If the environment is not filled yet, compile the wrapping call in place to
    /// back-infer (without recursing into the closure body, which is safe).
    fn enclosing_closure_params_for_decl(&mut self, decl: &SemanticId) -> Option<LuaType> {
        // Find decl's declaration -> if it is a Param, locate its closure and param index.
        let decls = self.model.decls()?;
        let decl = decls.iter().find(|d| &d.id == decl)?;
        if !matches!(decl.kind, DeclKind::Param) {
            return None;
        }
        let name = decl.name.clone();
        // Find the closure containing this param name (scan closures in the same file).
        let tree = self.model.syntax_tree()?;
        let chunk = tree.get_chunk_node();
        for closure in chunk.descendants::<LuaClosureExpr>() {
            let params = closure.get_params_list()?;

            for (index, param) in params.get_params().enumerate() {
                if let Some(token) = param.get_name_token()
                    && token.get_name_text() == name
                {
                    let closure_syntax = closure.get_syntax_id();
                    if let Some(bound) = self.closure_params.get(&(closure_syntax, index)) {
                        return Some(bound.clone());
                    }
                    // Environment not filled -> compile the wrapping call and back-infer arg types.
                    let bound = closure_param_vm(self.model, closure_syntax, index);
                    if !matches!(bound, LuaType::Unknown) {
                        self.closure_params
                            .insert((closure_syntax, index), bound.clone());
                        return Some(bound);
                    }
                    return None;
                }
            }
        }
        None
    }

    fn global_type(&self, owner: &SemanticId) -> LuaType {
        match self.model.resolve_owner(owner) {
            Some(SemanticId::Decl(decl)) => {
                let decl_id = SemanticId::Decl(decl);
                self.model
                    .type_of_decl(&decl_id)
                    .unwrap_or(LuaType::Unknown)
            }
            Some(SemanticId::TypeDef(def)) => {
                LuaType::Ref(LuaTypeDeclId::global(def.full_name.as_str()))
            }
            _ => LuaType::Unknown,
        }
    }

    /// Whether the file belongs to the STD workspace.
    fn is_std_file(model: &SemanticModel<'_>, file_id: FileId) -> bool {
        let Some(workspace) = model.db().workspace_input() else {
            return false;
        };
        query::file_workspace_id(model.db(), workspace, file_id) == Some(WorkspaceId::STD)
    }

    fn index_member_dynamic(&mut self, owner: Value, key: Value) -> Value {
        match &key.ty {
            LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
                self.index_member(owner, &LuaMemberKey::Name(SmolStr::new(s.as_str())))
            }
            LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
                if let LuaType::Array(array) = &owner.ty {
                    let base = array.get_base().clone();
                    return Value::plain(if self.model.db().strict_array_index() {
                        LuaType::from_vec(vec![base, LuaType::Nil])
                    } else {
                        base
                    });
                }
                self.index_member(owner, &LuaMemberKey::Integer(*i))
            }
            LuaType::Union(union) => {
                let mut types = Vec::new();
                let mut any_string = false;
                let mut any_numeric = false;
                for component in union.into_vec() {
                    match component {
                        LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
                            any_string = true;
                            let value = self.index_member(
                                owner.clone(),
                                &LuaMemberKey::Name(SmolStr::new(s.as_str())),
                            );
                            if !matches!(value.ty, LuaType::Unknown) {
                                if !types.contains(&value.ty) {
                                    types.push(value.ty);
                                }
                            }
                        }
                        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
                            any_numeric = true;
                            let value = self.index_member(owner.clone(), &LuaMemberKey::Integer(i));
                            if !matches!(value.ty, LuaType::Unknown) && !types.contains(&value.ty) {
                                types.push(value.ty);
                            }
                        }
                        LuaType::Number | LuaType::Integer | LuaType::FloatConst(_) => {
                            any_numeric = true;
                        }
                        LuaType::Nil => {}
                        _ => return Value::plain(LuaType::Unknown),
                    }
                }
                if let LuaType::Array(array) = &owner.ty {
                    if any_numeric
                        && !types
                            .iter()
                            .any(|ty| matches!(ty, LuaType::Nil) || ty == array.get_base())
                    {
                        types.push(array.get_base().clone());
                        if self.model.db().strict_array_index() {
                            types.push(LuaType::Nil);
                        }
                    }
                }
                if any_string && !types.is_empty() {
                    return Value::plain(LuaType::from_vec(types));
                }
                if any_numeric && !types.is_empty() {
                    return Value::plain(LuaType::from_vec(types));
                }
                Value::plain(LuaType::Unknown)
            }
            LuaType::Ref(_) | LuaType::Def(_) => {
                if is_enum_like_dynamic_key(self.model, &key.ty) {
                    // Enum member used as a key: resolve owner fields by each enum member name.
                    let Some(def) = self.model.type_def_of(match &key.ty {
                        LuaType::Ref(id) | LuaType::Def(id) => id,
                        _ => return Value::plain(LuaType::Unknown),
                    }) else {
                        return Value::plain(LuaType::Unknown);
                    };
                    let Some(enum_owner) = enum_runtime_owner(self.model, &def) else {
                        return Value::plain(LuaType::Unknown);
                    };
                    let mut types = Vec::new();
                    for member in self.model.members_of_owner(&enum_owner) {
                        let member_key = LuaMemberKey::Name(member.name.clone());
                        let value = self.index_member(owner.clone(), &member_key);
                        if !matches!(value.ty, LuaType::Unknown) && !types.contains(&value.ty) {
                            types.push(value.ty);
                        }
                    }
                    return if types.is_empty() {
                        Value::plain(LuaType::Unknown)
                    } else {
                        Value::plain(LuaType::from_vec(types))
                    };
                }
                if let Some(def) = self.model.type_def_of(match &key.ty {
                    LuaType::Ref(id) | LuaType::Def(id) => id,
                    _ => return Value::plain(LuaType::Unknown),
                }) && let Some(target) = self.model.alias_target(&def)
                {
                    return self.index_member_dynamic(owner, Value::plain(target));
                }
                Value::plain(LuaType::Unknown)
            }
            LuaType::String => self.index_member_dynamic_string(owner),
            LuaType::Number | LuaType::Integer | LuaType::FloatConst(_) => {
                if let LuaType::Array(array) = &owner.ty {
                    let base = array.get_base().clone();
                    return Value::plain(if self.model.db().strict_array_index() {
                        LuaType::from_vec(vec![base, LuaType::Nil])
                    } else {
                        base
                    });
                }
                // Dynamic integer key on a table literal: return the union of all
                // integer-keyed fields plus nil. This preserves exact shapes for runtime
                // integer indexing like `Pos[cur]` or `points[index]`.
                if let LuaType::TableConst(_) = &owner.ty {
                    let mut types = Vec::new();
                    for info in self.model.member_infos(&owner.ty) {
                        if matches!(info.key, LuaMemberKey::Integer(_))
                            && !types.contains(&info.typ)
                        {
                            types.push(info.typ.clone());
                        }
                    }
                    if !types.contains(&LuaType::Nil) {
                        types.push(LuaType::Nil);
                    }
                    return Value::plain(LuaType::from_vec(types));
                }
                if let LuaType::Union(union) = &owner.ty {
                    let mut types = Vec::new();
                    for component in union.into_vec() {
                        let component_owner = Value {
                            ty: component,
                            owner: owner.owner.clone(),
                            closure_syntax: None,
                            receiver: owner.receiver.clone(),
                        };
                        let result = self.index_member_dynamic(component_owner, key.clone());
                        if !matches!(result.ty, LuaType::Unknown) && !types.contains(&result.ty) {
                            types.push(result.ty);
                        }
                    }
                    return if types.is_empty() {
                        Value::plain(LuaType::Unknown)
                    } else {
                        Value::plain(LuaType::from_vec(types))
                    };
                }
                Value::plain(LuaType::Unknown)
            }
            _ => Value::plain(LuaType::Unknown),
        }
    }

    /// Dynamic string-key access on a table literal:
    /// with named fields, return the union of those field types plus nil; when there are
    /// only `[key]` computed keys (currently recorded as `Name("")`), return the index
    /// signature's value type without adding nil.
    fn index_member_dynamic_string(&mut self, owner: Value) -> Value {
        if !matches!(&owner.ty, LuaType::TableConst(_)) {
            return Value::plain(LuaType::Unknown);
        }
        let infos = self.model.member_infos(&owner.ty);
        let mut named_types = Vec::new();
        let mut empty_key_types = Vec::new();
        for info in infos {
            let LuaMemberKey::Name(name) = &info.key else {
                continue;
            };
            if name.is_empty() {
                if !empty_key_types.contains(&info.typ) {
                    empty_key_types.push(info.typ.clone());
                }
            } else if !named_types.contains(&info.typ) {
                named_types.push(info.typ.clone());
            }
        }
        if !named_types.is_empty() {
            if !named_types.contains(&LuaType::Nil) {
                named_types.push(LuaType::Nil);
            }
            return Value::plain(LuaType::from_vec(named_types));
        }
        if !empty_key_types.is_empty() {
            return Value::plain(LuaType::from_vec(empty_key_types));
        }
        Value::plain(LuaType::Nil)
    }

    /// Whether an anonymous table literal already has a known member shape.
    /// An empty `{}` with no fields/runtime members is treated as "unknown shape"; missing
    /// keys return Unknown instead of a definite Nil so not-yet-initialized members are not
    /// treated as missing (issue 600).
    fn table_literal_has_members(model: &SemanticModel<'_>, ty: &LuaType) -> bool {
        let LuaType::TableConst(table) = ty else {
            return true;
        };
        let table_owner = SemanticId::member(table.file_id, table.value);
        if !model.members_of_owner(&table_owner).is_empty() {
            return true;
        }
        if let Some(facts) = model.file_facts_of(table.file_id) {
            for decl in &facts.decls {
                if decl
                    .value_expr_syntax
                    .is_some_and(|syntax| syntax.get_range() == table.value)
                    && !model.members_of_owner(&decl.id).is_empty()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Find a generic param's constraint type by name in the current file's signatures
    /// (`---@generic T: Base` -> `Base`).
    fn generic_constraint_by_name(&self, name: &str) -> Option<LuaType> {
        let signatures = self.model.signatures()?;
        for sig in signatures {
            let docs = sig.docs.as_ref()?;
            for param in &docs.generic_params {
                if param.name == name {
                    let constraint = param.constraint?;
                    return Some(
                        self.model
                            .doc_type_lua_rich_in(self.model.file_id(), constraint),
                    );
                }
            }
        }
        None
    }

    fn index_member(&mut self, mut owner: Value, key: &LuaMemberKey) -> Value {
        let receiver_ty = Some(owner.ty.clone());
        // On `any` (including alias cycles that converge to any), any field is still any.
        if owner.ty.is_any() {
            return Value::plain(LuaType::Any);
        }
        // Member access on generic param `T: Base`: use the constraint type as the
        // member-lookup owner.
        if let LuaType::TplRef(tpl) = &owner.ty
            && let Some(constraint) = tpl.get_constraint()
        {
            owner.ty = constraint.clone();
        } else if let LuaType::Ref(id) = &owner.ty
            && let Some(constraint) = self.generic_constraint_by_name(id.get_name())
        {
            owner.ty = constraint;
        }
        // Candidate owner identities: table identity (TableConst synthetic (file, range))
        // is preferred for nested-table field ownership; then the declaration owner
        // (`local t = {}; t.a = 5` members belong to the Decl).
        let mut owners: Vec<SemanticId> = Vec::new();
        if let LuaType::TableConst(table) = &owner.ty {
            owners.push(SemanticId::member(table.file_id, table.value));
        }
        if let Some(owner_id) = &owner.owner
            && !owners.contains(owner_id)
        {
            owners.push(owner_id.clone());
        }
        // Runtime members of global declarations are collected under the `Name(path)` key
        // (`math.min`); even when the declaration is associated with `---@class mathlib`,
        // the Name identity must be retained.
        if let Some(SemanticId::Decl(decl_key)) = &owner.owner
            && let Some(facts) = self.model.file_facts_of(decl_key.file_id)
            && let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_key.clone()))
            && matches!(decl.kind, DeclKind::Global)
        {
            let name_owner = SemanticId::Name(decl.name.clone().into());
            if !owners.contains(&name_owner) {
                owners.push(name_owner);
            }
        }
        // `local x = {}` associated with `---@class MyClass`: also add the class def as a
        // member candidate so `@field` typed members (e.g. `unpack` overloads) are
        // reachable through the table identity. The runtime table and class name may differ;
        // only the annotation and declaration need to share the same owner_syntax.
        if let Some(SemanticId::Decl(decl_key)) = &owner.owner
            && let Some(facts) = self.model.file_facts_of(decl_key.file_id)
            && let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_key.clone()))
            && let Some(def) = facts.type_defs.iter().find(|def| {
                def.owner_syntax.is_some()
                    && def.owner_syntax == decl.owner_syntax
                    && matches!(def.kind, TypeDefKind::Class | TypeDefKind::Enum)
            })
        {
            let type_def_id = def.id.clone();
            if !owners.contains(&type_def_id) {
                owners.push(type_def_id);
            }
        }
        // When a global class table is written directly as
        // `function GlobalClass:method()`, the method owner is `Name("GlobalClass")`. Only
        // add it when accessing through the class table itself (same-named Decl/Name), to
        // avoid leaking class-table runtime fields into arbitrary instances
        // (`local other: Foo` should not see `Foo.extra`).
        if let LuaType::Ref(id) | LuaType::Def(id) = &owner.ty
            && let Some(def) = self.model.type_def_of(id)
            && let Some(owner_id) = &owner.owner
            && owner_is_class_table_name(self.model, owner_id, def.name.as_str())
        {
            let name_owner = SemanticId::name(def.name.clone());
            if !owners.contains(&name_owner) {
                owners.push(name_owner);
            }
        }
        // Declared named-type members take precedence over runtime members on the
        // table/instance: `b["field"]` for `local b: B = { field = 1 }` should use the
        // class `@field field integer`, not project the table literal `1` as `IntegerConst`.
        let type_has_member = matches!(
            &owner.ty,
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_)
        ) && self.model.member_type(&owner.ty, key).is_some();
        for owner_id in owners {
            if type_has_member {
                continue;
            }
            // Cross-file member association.
            let members = self.model.members_of_owner(&owner_id);
            if let Some(member) = members.into_iter().find(|m| match key {
                LuaMemberKey::Name(name) => &m.name == name,
                LuaMemberKey::Integer(_) => self
                    .model
                    .file_facts_of(m.file_id)
                    .and_then(|facts| facts.member_by_id(&m.id))
                    .is_some_and(|member| &member.key == key),
                _ => false,
            }) {
                if self.computing.contains(&member.id) {
                    return Value::plain(LuaType::Unknown);
                }
                self.computing.insert(member.id.clone());
                let mut ty = self
                    .model
                    .type_of_member(&member.id)
                    .unwrap_or(LuaType::Unknown);
                // For a table literal's computed key `[key] = 1` (currently recorded as
                // `Name("")`), preserve the literal type from construction in dynamic index;
                // named fields still go through the existing projection (`M.y = 1` stays Number).
                if matches!(&owner.ty, LuaType::TableConst(_))
                    && let Some(facts) = self.model.file_facts_of(member.file_id)
                    && let Some(member_def) = facts.member_by_id(&member.id)
                    && matches!(&member_def.key, LuaMemberKey::Name(name) if name.is_empty())
                    && let Some(value_syntax) = member_def.value_syntax
                {
                    let expr_ty = self.model.type_of_expr(value_syntax);
                    if !matches!(expr_ty, LuaType::Unknown) {
                        ty = expr_ty;
                    }
                }
                // When member-type projection fails, fall back to resolving by the prefix
                // type (common for stdlib global library members).
                if matches!(ty, LuaType::Unknown) {
                    let lua_key = key.clone();
                    if let Some(fallback) = self.model.member_type(&owner.ty, &lua_key) {
                        ty = fallback;
                    }
                }
                // If still a broad Function/Unknown, project a DocFunction from the member
                // closure expression.
                if matches!(ty, LuaType::Unknown | LuaType::Function)
                    && let Some(facts) = self.model.file_facts_of(member.file_id)
                    && let Some(member_def) = facts.member_by_id(&member.id)
                    && let Some(value_syntax) = member_def.value_syntax
                {
                    let expr_ty = self.model.type_of_expr(value_syntax);
                    if !matches!(expr_ty, LuaType::Unknown) {
                        ty = expr_ty;
                    }
                }
                self.computing.remove(&member.id);
                return Value {
                    ty,
                    owner: Some(member.id),
                    closure_syntax: None,
                    receiver: receiver_ty.clone(),
                };
            }
        }
        // Prefix-type member fallback: when no member is found under the owner syntax
        // identity, resolve by **type** (named type `@field` / inheritance chain / union
        // components, e.g. `t: T` with fields defined on T).
        let lua_key = key.clone();
        let member_type_fallback = self.model.member_type(&owner.ty, &lua_key);

        // Repeated `---@field event fun(...)` denotes overloads: multiple callable members
        // with the same key should be merged into a union so call inference can select a
        // candidate by args (a single member still keeps its original broad
        // Function/DocFunction projection).
        if let Some(overloaded_ty) = self.overloaded_callable_member_type(&owner.ty, &lua_key) {
            return Value {
                ty: overloaded_ty,
                owner: None,
                closure_syntax: None,
                receiver: receiver_ty.clone(),
            };
        }

        // When an exact type already exists, use it directly (preserves old behavior and
        // avoids breaking generic-constructor resolution). Stdlib globals/members need to
        // keep the Member owner for dispatch, so do not return early. The decision is based
        // on "declaration file belongs to the STD workspace", not a hardcoded name list.
        let is_std_global = matches!(&owner.owner, Some(SemanticId::Decl(key)) if {
            Self::is_std_file(self.model, key.file_id)
        });
        let needs_member_owner =
            matches!(member_type_fallback, Some(LuaType::Function)) && is_std_global;
        // Ordinary methods on named instances/class tables are projected to a broad
        // `Function` by type-side member query; the member identity must still be carried
        // back so call inference can parse `---@return` from the member signature.
        // `Signature` also needs the member identity (especially stdlib `---@class mathlib`
        // + `math = {}` runtime tables associated with the class).
        let needs_method_owner = self
            .model
            .member_info(&owner.ty, &lua_key)
            .and_then(|info| info.id)
            .is_some()
            && (matches!(member_type_fallback, Some(LuaType::Signature(_)))
                || (matches!(member_type_fallback, Some(LuaType::Function)) && !is_std_global));
        if let Some(ty) = &member_type_fallback
            && !matches!(ty, LuaType::Unknown)
            && !needs_member_owner
            && !needs_method_owner
        {
            // When a type-side member is projected to a broad Function, look up the full
            // signature only for members with method-level generics (e.g. `---@generic U = T`
            // method defaults need signature metadata). Ordinary class methods stay broad
            // Function to avoid disturbing existing param/return checks.
            if matches!(ty, LuaType::Function)
                && let Some(info) = self.model.member_info(&owner.ty, &lua_key)
                && let Some(member_id) = info.id
                && let Some(file_id) = info.file_id
                && let Some(facts) = self.model.file_facts_of(file_id)
                && let Some(member_def) = facts.member_by_id(&member_id)
                && let Some(value_syntax) = member_def.value_syntax
                && facts
                    .signature_by_closure(value_syntax)
                    .and_then(|sig| sig.docs.as_ref())
                    .is_some_and(|docs| !docs.generic_params.is_empty())
            {
                if let Some(fun) = self.model.type_of_signature_in_file(file_id, value_syntax) {
                    return Value {
                        ty: LuaType::DocFunction(Arc::new(fun)),
                        owner: Some(member_id),
                        closure_syntax: None,
                        receiver: receiver_ty.clone(),
                    };
                }
                let expr_ty = self.model.type_of_expr(value_syntax);
                if !matches!(expr_ty, LuaType::Unknown) {
                    return Value {
                        ty: expr_ty,
                        owner: Some(member_id),
                        closure_syntax: None,
                        receiver: receiver_ty.clone(),
                    };
                }
            }
            return Value {
                ty: ty.clone(),
                owner: None,
                closure_syntax: None,
                receiver: receiver_ty.clone(),
            };
        }
        if needs_method_owner
            && let Some(info) = self.model.member_info(&owner.ty, &lua_key)
            && let Some(member_id) = info.id
        {
            return Value {
                ty: member_type_fallback.clone().unwrap_or(LuaType::Function),
                owner: Some(member_id),
                closure_syntax: None,
                receiver: receiver_ty.clone(),
            };
        }

        // Members of global tables (`math` / `table`) often hang on the global Name owner:
        // look up the Name owner from the TableConst's corresponding Decl name.
        if let LuaType::TableConst(_table) = &owner.ty
            && let Some(SemanticId::Decl(decl_key)) = &owner.owner
            && let Some(facts) = self.model.file_facts_of(decl_key.file_id)
            && let Some(decl) = facts.decl_by_id(&SemanticId::Decl(decl_key.clone()))
        {
            let name_owner = SemanticId::Name(decl.name.clone().into());
            let members = self.model.members_of_owner(&name_owner);
            let member = match key {
                LuaMemberKey::Name(name) => members.into_iter().find(|m| &m.name == name),
                LuaMemberKey::Integer(_) => members.into_iter().find(|m| {
                    self.model
                        .file_facts_of(m.file_id)
                        .and_then(|facts| facts.member_by_id(&m.id))
                        .is_some_and(|member| &member.key == key)
                }),
                _ => None,
            };
            if let Some(member) = member {
                let mut ty = self
                    .model
                    .type_of_member(&member.id)
                    .unwrap_or(LuaType::Unknown);
                if matches!(ty, LuaType::Unknown | LuaType::Function)
                    && let Some(facts) = self.model.file_facts_of(member.file_id)
                    && let Some(member_def) = facts.member_by_id(&member.id)
                    && let Some(value_syntax) = member_def.value_syntax
                {
                    ty = self.model.type_of_expr(value_syntax);
                }
                return Value {
                    ty,
                    owner: Some(member.id),
                    closure_syntax: None,
                    receiver: receiver_ty.clone(),
                };
            }
        }
        if let Some(ty) = member_type_fallback {
            return Value {
                ty,
                owner: None,
                closure_syntax: None,
                receiver: receiver_ty,
            };
        }
        // Named table literals / declared class instances return nil for missing keys (Lua
        // member-index semantics), not Unknown; `t.missing` / `other.extra` should not
        // degrade to "cannot infer". For empty table literals with no known member shape,
        // still return Unknown because whether such fields are missing cannot be decided
        // from the type side, and nil would treat not-yet-initialized members as definitely
        // missing (issue 600).
        let missing_is_known = match &owner.ty {
            LuaType::TableConst(_) => Self::table_literal_has_members(self.model, &owner.ty),
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Object(_) => true,
            _ => false,
        };
        if missing_is_known {
            return Value::plain(LuaType::Nil);
        }
        Value::plain(LuaType::Unknown)
    }

    /// Merge multiple callable members with the same key into a union (repeated `@field`
    /// overloads). Only merge when every same-key member is callable, to avoid changing the
    /// first-member semantics of ordinary repeated fields.
    fn overloaded_callable_member_type(
        &self,
        owner_ty: &LuaType,
        key: &LuaMemberKey,
    ) -> Option<LuaType> {
        let infos =
            crate::semantic_model::member::member_infos_with_key_all(self.model, owner_ty, key);
        let mut types = Vec::new();
        for info in &infos {
            if !matches!(info.typ, LuaType::DocFunction(_)) {
                return None;
            }
            if !types.contains(&info.typ) {
                types.push(info.typ.clone());
            }
        }
        if types.len() > 1 {
            Some(LuaType::from_vec(types))
        } else {
            None
        }
    }

    /// Resolve a built-in global function name: a global Name or a same-named Decl declared
    /// in the std lib. Follows local alias chains (`local runner = pcall`) but does not
    /// hijack user local functions.
    fn callee_builtin_name(&self, callee: &Value) -> Option<SmolStr> {
        match &callee.owner {
            Some(SemanticId::Name(name)) => Some(SmolStr::new(name.as_str())),
            Some(owner @ SemanticId::Decl(_)) => {
                let mut visited = HashSet::new();
                let mut current = owner.clone();
                loop {
                    if !visited.insert(current.clone()) {
                        return None;
                    }
                    let SemanticId::Decl(key) = &current else {
                        return None;
                    };
                    let facts = self.model.file_facts_of(key.file_id)?;
                    let decl = facts.decl_by_id(&current)?;
                    match decl.kind {
                        DeclKind::Global => return Some(decl.name.clone()),
                        DeclKind::Local { .. } => {
                            let value_syntax = decl.value_expr_syntax?;
                            let tree = self.model.syntax_tree_of(key.file_id)?;
                            let node = value_syntax.to_node_from_root(&tree.get_red_root())?;
                            let LuaExpr::NameExpr(name_expr) = LuaExpr::cast(node)? else {
                                return None;
                            };
                            current = self.model.resolve_name(name_expr.get_position())?;
                        }
                        _ => return None,
                    }
                }
            }
            _ => None,
        }
    }

    /// Resolve a member call name (`table.unpack` -> `unpack`).
    /// Only enable built-in special cases for members of global named tables
    /// (stdlib/global objects), so user-class methods are not hijacked by names like
    /// `unpack`/`pcall`.
    fn callee_member_name(&self, callee: &Value) -> Option<SmolStr> {
        let SemanticId::Member(key) = callee.owner.as_ref()? else {
            return None;
        };
        let facts = self.model.file_facts_of(key.file_id)?;
        let member = facts.member_by_id(callee.owner.as_ref()?)?;
        if !matches!(member.owner, SemanticId::Name(_)) {
            return None;
        }
        Some(member.key.to_path().into())
    }

    fn call(
        &mut self,
        callee: Value,
        args: &[Value],
        colon_call: bool,
        generic_type_syntaxes: &[LuaSyntaxId],
    ) -> Value {
        // Calling an `any` callable still yields `any` (TS any propagation semantics).
        if callee.ty.is_any() {
            return Value::plain(LuaType::Any);
        }
        // Global built-in special cases (no signature to look up; dispatch by name).
        let builtin_name = self
            .callee_builtin_name(&callee)
            .or_else(|| self.callee_member_name(&callee));
        if let Some(name) = builtin_name {
            match name.as_str() {
                // setmetatable(t, mt) -> t; if mt.__index is a named type/class table, the
                // instance directly takes that type.
                "setmetatable" => {
                    let Some(first) = args.first() else {
                        return Value::plain(LuaType::Unknown);
                    };
                    if let Some(metatable) = args.get(1) {
                        let index_key = LuaMemberKey::Name(SmolStr::new("__index"));
                        if let Some(info) = self.model.member_info(&metatable.ty, &index_key) {
                            let index_ty = match &info.typ {
                                LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
                                    Some(info.typ.clone())
                                }
                                LuaType::TableConst(index_table)
                                    if crate::semantic_model::member::table_const_class_type(
                                        self.model,
                                        index_table,
                                    )
                                    .is_some() =>
                                {
                                    crate::semantic_model::member::table_const_class_type(
                                        self.model,
                                        index_table,
                                    )
                                }
                                _ => None,
                            };
                            if let Some(index_ty) = index_ty {
                                return Value {
                                    ty: index_ty,
                                    owner: first.owner.clone(),
                                    closure_syntax: None,
                                    receiver: first.receiver.clone(),
                                };
                            }
                        }
                    }
                    return first.clone();
                }
                // require("mod") / require(mod_var): string constant module name -> module export type.
                "require" => {
                    if let Some(module_name) = string_const_of(
                        &args
                            .first()
                            .map(|a| a.ty.clone())
                            .unwrap_or(LuaType::Unknown),
                    ) {
                        return Value::plain(self.model.require_module_type(&module_name));
                    }
                }
                // assert(x, ...): returns x when truthy (removing nil) and passes through
                // any following multi-returns.
                "assert" => {
                    if let Some(arg) = args.first() {
                        let ty = remove_nil_from_type(arg.ty.clone());
                        return Value::plain(ty);
                    }
                    return Value::plain(LuaType::Unknown);
                }
                // pcall(f, ...): true/false + callback returns; the VM only provides the
                // first boolean slot, the rest come from the callback return.
                "pcall" | "xpcall" => {
                    let callback_ret = args.first().and_then(|arg| {
                        let call_args: Vec<super::overload::CallArg> = args[1..]
                            .iter()
                            .map(|a| super::overload::CallArg {
                                ty: a.ty.clone(),
                                closure_syntax: a.closure_syntax,
                            })
                            .collect();
                        super::overload::pcall_callback_ret(
                            self.model,
                            &arg.ty,
                            arg.owner.as_ref(),
                            arg.closure_syntax,
                            &call_args,
                        )
                    });
                    if let Some((callback_ret, include_error_string)) = callback_ret {
                        return Value::plain(super::overload::pcall_return_type(
                            callback_ret,
                            include_error_string,
                        ));
                    }
                    return Value::plain(LuaType::Boolean);
                }
                // table.unpack(t): expand a table literal's integer-key members into
                // multi-returns in order.
                "unpack" => {
                    let Some(arg) = args.first() else {
                        return Value::plain(LuaType::Unknown);
                    };
                    if let LuaType::TableConst(table) = &arg.ty {
                        let owner = SemanticId::member(table.file_id, table.value);
                        let members = self.model.members_of_owner(&owner);
                        let mut indexed: Vec<(i64, LuaType)> = Vec::new();
                        for m in members {
                            if let Some(facts) = self.model.file_facts_of(m.file_id)
                                && let Some(member) = facts.member_by_id(&m.id)
                            {
                                if let LuaMemberKey::Integer(i) = &member.key {
                                    let ty = if let Some(value_syntax) = member.value_syntax {
                                        self.model.type_of_expr(value_syntax)
                                    } else {
                                        self.model.type_of_member(&m.id).unwrap_or(LuaType::Unknown)
                                    };
                                    indexed.push((*i, ty));
                                }
                            }
                        }
                        indexed.sort_by_key(|(i, _)| *i);
                        let types = indexed.into_iter().map(|(_, ty)| ty).collect::<Vec<_>>();
                        return Value::plain(LuaType::Variadic(Arc::new(VariadicType::Multi(
                            types,
                        ))));
                    }
                    // `table.unpack(number[])`: array elements can contain nil, so return
                    // unbounded `number?`; the global `unpack` (Lua 5.1) treats the table as
                    // a dense array in table constructors and does not add extra nil.
                    if let LuaType::Array(array) = &arg.ty {
                        let base = array.get_base().clone();
                        let element = if matches!(callee.owner, Some(SemanticId::Member(_))) {
                            LuaType::from_vec(vec![base, LuaType::Nil])
                        } else {
                            base
                        };
                        return Value::plain(LuaType::Variadic(Arc::new(VariadicType::Base(
                            element,
                        ))));
                    }
                    return Value::plain(LuaType::Unknown);
                }
                // select(n, ...) / select('#', ...)
                "select" => {
                    // `select('#', ...)`: returns the number of remaining args.
                    if let Some(first) = args.first() {
                        if let LuaType::StringConst(s) | LuaType::DocStringConst(s) = &first.ty {
                            if s.as_str() == "#" {
                                let mut count = 0usize;
                                let mut has_unbounded = false;
                                for arg in args.iter().skip(1) {
                                    match &arg.ty {
                                        LuaType::Variadic(variadic) => match variadic.as_ref() {
                                            VariadicType::Multi(types) => count += types.len(),
                                            VariadicType::Base(_) => has_unbounded = true,
                                        },
                                        _ => count += 1,
                                    }
                                }
                                if has_unbounded {
                                    return Value::plain(LuaType::Integer);
                                }
                                return Value::plain(LuaType::IntegerConst(count as i64));
                            }
                        }
                    }

                    let n = args
                        .first()
                        .and_then(|arg| match arg.ty {
                            LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
                                Some(i as usize)
                            }
                            _ => None,
                        })
                        .unwrap_or(1)
                        .max(1);
                    let mut tail: Vec<LuaType> = Vec::new();
                    let mut has_unbounded = false;
                    let mut remaining = n.saturating_sub(1);
                    for arg in args.iter().skip(1) {
                        match &arg.ty {
                            LuaType::Variadic(variadic) => match variadic.as_ref() {
                                VariadicType::Multi(types) => {
                                    for ty in types {
                                        if remaining > 0 {
                                            remaining -= 1;
                                        } else {
                                            tail.push(widen_const(ty));
                                        }
                                    }
                                }
                                VariadicType::Base(base) => {
                                    // Unbounded variadic: once entered, all later values are
                                    // the base type.
                                    if remaining > 0 {
                                        remaining = 0;
                                    }
                                    tail.push(widen_const(base));
                                    has_unbounded = true;
                                }
                            },
                            ty => {
                                if remaining > 0 {
                                    remaining -= 1;
                                } else {
                                    tail.push(widen_const(ty));
                                }
                            }
                        }
                    }
                    if has_unbounded {
                        let base = tail.first().cloned().unwrap_or(LuaType::Unknown);
                        return Value::plain(LuaType::Variadic(Arc::new(VariadicType::Base(base))));
                    }
                    if tail.is_empty() {
                        return Value::plain(LuaType::Nil);
                    }
                    return Value::plain(LuaType::Variadic(Arc::new(VariadicType::Multi(tail))));
                }
                _ => {}
            }
        }

        // Collect candidate signatures: declaration/member docs (including overloads) are
        // preferred, then the function value type; named aliases (`A = A | fun(): integer`)
        // are expanded to get inner function candidates.
        let candidates = self.callable_candidates(&callee);
        // For class-table receivers (`local a = {}` + `---@class a`), pass the class
        // reference type to implicit self/constructor params instead of the bare TableConst,
        // otherwise `a:create()` binds T to table.
        let match_receiver = callee.receiver.as_ref().map(|receiver| {
            receiver_class_type(self.model, receiver).unwrap_or_else(|| receiver.clone())
        });
        let Some((callee_fun, mut bindings)) = self
            .select_callable(&candidates, args, colon_call, match_receiver.as_ref())
            .or_else(|| {
                self.select_callable_partial(&candidates, args, colon_call, match_receiver.as_ref())
            })
        else {
            return Value::plain(LuaType::Unknown);
        };

        // Explicit call generics: `f--[[@<string, number>]]()` binds directly in function
        // generic-param order.
        for (index, syntax) in generic_type_syntaxes.iter().enumerate() {
            if let Some(tpl) = callee_fun.get_generic_params().get(index) {
                let ty = self
                    .model
                    .doc_type_lua_in(self.model.file_id(), *syntax, &[]);
                bindings.insert(tpl.get_tpl_id(), ty);
            }
        }

        // Fill unresolved function generics with defaults/constraints; class-method defaults
        // may reference class generics (`U = T`).
        let class_map = callee
            .receiver
            .as_ref()
            .and_then(|receiver| {
                crate::semantic_model::type_eval::class_generic_map(self.model, receiver)
            })
            .unwrap_or_default();
        for param in callee_fun.get_generic_params() {
            if bindings.contains_key(&param.get_tpl_id()) {
                continue;
            }
            if let Some(default) = param.get_default_type() {
                let default =
                    crate::semantic_model::type_eval::substitute_named_refs(default, &class_map);
                bindings.insert(param.get_tpl_id(), default);
            } else if let Some(constraint) = param.get_constraint() {
                let constraint =
                    crate::semantic_model::type_eval::substitute_named_refs(constraint, &class_map);
                bindings.insert(param.get_tpl_id(), constraint);
            }
        }
        // Defaults may reference earlier generics (`U = T[]`); iteratively substitute until stable.
        for _ in 0..callee_fun.get_generic_params().len().max(1) {
            let mut changed = false;
            let mut name_bindings: HashMap<String, LuaType> = class_map.clone();
            for param in callee_fun.get_generic_params() {
                if let Some(value) = bindings.get(&param.get_tpl_id()) {
                    name_bindings.insert(param.get_name().to_string(), value.clone());
                }
            }
            for param in callee_fun.get_generic_params() {
                if let Some(value) = bindings.get(&param.get_tpl_id()).cloned() {
                    let new_value = crate::semantic_model::type_eval::substitute_named_refs(
                        &value,
                        &name_bindings,
                    );
                    let new_value = unify::substitute(&new_value, &bindings);
                    if new_value != value {
                        changed = true;
                    }
                    bindings.insert(param.get_tpl_id(), new_value);
                }
            }
            if !changed {
                break;
            }
        }

        // The first param of a colon-called member function type is often `self`, which is
        // absent from the actual arg list. Closure arg indices must align with function
        // param indices (e.g. `obj:event(fn)`'s fn corresponds to param 1).
        let callback_param_offset = if colon_call
            && callee_fun
                .get_params()
                .first()
                .is_some_and(|(name, ty)| name == "self" || matches!(ty, Some(LuaType::SelfInfer)))
        {
            1
        } else {
            0
        };
        let receiver_ty = callee.receiver.clone();

        // Closure args: substitute the callee's function-typed params and fill the param environment.
        for (index, arg) in args.iter().enumerate() {
            if let Some(closure_syntax) = arg.closure_syntax {
                let Some(callback_param_ty) = callee_fun
                    .get_params()
                    .get(index + callback_param_offset)
                    .and_then(|p| p.1.clone())
                else {
                    continue;
                };
                let callback_param_ty =
                    bind_signature_generics(&callback_param_ty, callee_fun.get_generic_params());
                let callback_ty = unify::substitute(&callback_param_ty, &bindings);
                let Some(callback_fun) = callback_fun_from_param(self.model, &callback_ty) else {
                    continue;
                };
                for (param_idx, callback_param) in callback_fun.get_params().iter().enumerate() {
                    if let Some(param_ty) = &callback_param.1 {
                        let mut substituted = unify::substitute(param_ty, &bindings);
                        if let Some(self_ty) = &receiver_ty {
                            substituted = replace_self_type(&substituted, self_ty);
                        }
                        if let LuaType::Variadic(variadic) = &substituted {
                            let slot_types: Vec<LuaType> = match variadic.as_ref() {
                                VariadicType::Base(base) => {
                                    if let LuaType::Tuple(tuple) = base {
                                        tuple.get_types().to_vec()
                                    } else {
                                        vec![base.clone()]
                                    }
                                }
                                VariadicType::Multi(types) => types.clone(),
                            };
                            for (offset, slot_ty) in slot_types.iter().enumerate() {
                                self.closure_params
                                    .insert((closure_syntax, param_idx + offset), slot_ty.clone());
                            }
                        } else {
                            self.closure_params
                                .insert((closure_syntax, param_idx), substituted);
                        }
                    }
                }
            }
        }
        // Table-literal args (`fnA({ hook = function(obj) ... end })`): fill params from the
        // expected object field signatures.
        for (index, arg) in args.iter().enumerate() {
            if !matches!(arg.ty, LuaType::TableConst(_)) {
                continue;
            }
            let Some(param_ty) = callee_fun
                .get_params()
                .get(index + callback_param_offset)
                .and_then(|p| p.1.clone())
            else {
                continue;
            };
            let param_ty = bind_signature_generics(&param_ty, callee_fun.get_generic_params());
            let param_ty = unify::substitute(&param_ty, &bindings);
            if !matches!(param_ty, LuaType::Unknown | LuaType::Any) {
                self.bind_table_literal_closure_params(&arg.ty, &param_ty, &bindings);
            }
        }
        // Closure-arg return back-inference: after the param environment is filled, use the
        // real closure body return type to complete callback return generics (e.g. in
        // `map(list, function(item) return item end)`, U = T = string).
        for (index, arg) in args.iter().enumerate() {
            let Some(closure_syntax) = arg.closure_syntax else {
                continue;
            };
            let Some(callback_param_ty) = callee_fun
                .get_params()
                .get(index + callback_param_offset)
                .and_then(|p| p.1.clone())
            else {
                continue;
            };
            let callback_param_ty =
                bind_signature_generics(&callback_param_ty, callee_fun.get_generic_params());
            let callback_ty = unify::substitute(&callback_param_ty, &bindings);
            if let LuaType::DocFunction(callback_fun) = callback_ty {
                // `fun(): R...` variadic returns are handled uniformly by call/return checks;
                // closure-body inference tends to merge multi-return slots into a single R, so
                // skip higher-order return inference here.
                if matches!(callback_fun.get_ret(), LuaType::Variadic(_)) {
                    continue;
                }
                let closure_ret = self.closure_return_type_with_env(closure_syntax);
                if !matches!(closure_ret, LuaType::Unknown | LuaType::Any) {
                    let _ = unify_call_bindings(
                        self.model,
                        callback_fun.get_ret(),
                        &closure_ret,
                        &mut bindings,
                    );
                }
            }
        }
        // apply / higher-order callback return back-inference: "call" the actual callback
        // value with the later args and use the real return type to complete callback return
        // generics (`apply(run, cb)`'s R = run(cb)).
        for (index, arg) in args.iter().enumerate() {
            let Some(callback_param_ty) = callee_fun
                .get_params()
                .get(index + callback_param_offset)
                .and_then(|p| p.1.clone())
            else {
                continue;
            };
            let callback_param_ty =
                bind_signature_generics(&callback_param_ty, callee_fun.get_generic_params());
            let LuaType::DocFunction(original_callback_fun) = &callback_param_ty else {
                continue;
            };
            let original_callback_ret = original_callback_fun.get_ret().clone();
            let callback_ty = unify::substitute(&callback_param_ty, &bindings);
            let LuaType::DocFunction(callback_fun) = callback_ty else {
                continue;
            };
            // Call the actual callback value (only with the later args) and bind its result
            // to the callback return. Closure literals also participate in overload selection
            // via their signatures and follow the same apply path as named callbacks.
            let callback_value_ty = if let Some(closure_syntax) = arg.closure_syntax {
                self.model
                    .type_of_signature(closure_syntax)
                    .map(|fun| LuaType::DocFunction(Arc::new(fun)))
                    .unwrap_or_else(|| arg.ty.clone())
            } else {
                arg.ty.clone()
            };
            let callback_value = Value {
                ty: callback_value_ty,
                owner: arg.owner.clone(),
                closure_syntax: None,
                receiver: arg.receiver.clone(),
            };
            let callback_args = args.get(index + 1..).unwrap_or(&[]);
            if self.callback_call_depth >= 8 {
                continue;
            }
            // Prefer return inference with the filled param environment for closure literals:
            // it sees `---@as` / flow casts inside the body, avoiding the apply path using
            // bare VM table identity to overwrite callback return generics. Variadic returns
            // (`R...`) still go through the apply path to preserve multi-return slot semantics.
            let callback_ret = if let Some(closure_syntax) = arg.closure_syntax
                && !matches!(original_callback_ret, LuaType::Variadic(_))
            {
                let ret = self.closure_return_type_with_env(closure_syntax);
                if !matches!(ret, LuaType::Unknown | LuaType::Any) {
                    ret
                } else {
                    self.callback_call_depth += 1;
                    let ret = self.call(callback_value.clone(), callback_args, false, &[]);
                    self.callback_call_depth -= 1;
                    ret.ty
                }
            } else {
                self.callback_call_depth += 1;
                let ret = self.call(callback_value.clone(), callback_args, false, &[]);
                self.callback_call_depth -= 1;
                ret.ty
            };
            if !matches!(callback_ret, LuaType::Unknown | LuaType::Any) {
                self.bind_callback_return(
                    &original_callback_ret,
                    callback_fun.get_ret(),
                    &callback_ret,
                    &mut bindings,
                );
            } else {
                // Returning Unknown may also mean a structural generic overload was selected
                // (e.g. `fun(): unknown`). Only override the fallback when the actual callback
                // has a matching structural (DocFunction-param) overload.
                let call_args: Vec<super::overload::CallArg> = callback_args
                    .iter()
                    .map(|a| super::overload::CallArg {
                        ty: a.ty.clone(),
                        closure_syntax: a.closure_syntax,
                    })
                    .collect();
                let has_structural_overload =
                    self.callable_candidates(&callback_value).iter().any(|fun| {
                        fun.get_params()
                            .iter()
                            .any(|(_, ty)| matches!(ty, Some(LuaType::DocFunction(_))))
                            && super::overload::match_call_candidate(
                                self.model, fun, &call_args, false, None,
                            )
                            .is_some()
                    });
                if has_structural_overload {
                    self.bind_callback_return(
                        &original_callback_ret,
                        callback_fun.get_ret(),
                        &LuaType::Unknown,
                        &mut bindings,
                    );
                }
            }
        }

        let mut ret = self.substituted_call_return(&callee, &callee_fun, &bindings);
        // Directly callable unions: when multiple same-priority candidates match (union types
        // or ordinary `---@overload` in higher-order callback return projections), take the
        // union of return types. Ordinary direct calls still keep the first best candidate,
        // avoiding stdlib overloads (e.g. math.min) unexpectedly becoming multi-return unions.
        if (self.callback_call_depth > 0 || matches!(callee.ty, LuaType::Union(_)))
            && candidates.len() > 1
        {
            let call_args: Vec<super::overload::CallArg> = args
                .iter()
                .map(|arg| super::overload::CallArg {
                    ty: arg.ty.clone(),
                    closure_syntax: arg.closure_syntax,
                })
                .collect();
            let all_best = super::overload::select_callable_all(
                self.model,
                &candidates,
                &call_args,
                colon_call,
                match_receiver.as_ref(),
            );
            if all_best.len() > 1 {
                let mut returns = Vec::new();
                for (fun, bindings) in &all_best {
                    let candidate_ret = self.substituted_call_return(&callee, fun, bindings);
                    if !returns.contains(&candidate_ret) {
                        returns.push(candidate_ret);
                    }
                }
                ret = if returns.len() == 1 {
                    returns.pop().expect("len checked")
                } else {
                    LuaType::from_vec(returns)
                };
            }
        }
        // Class generics directly referenced in method returns (e.g. T in `T[K]`) must be
        // substituted by the receiver instance too.
        let ret = callee
            .receiver
            .as_ref()
            .and_then(|receiver| {
                crate::semantic_model::type_eval::class_generic_map(self.model, receiver)
            })
            .map(|class_map| {
                crate::semantic_model::type_eval::substitute_named_refs(&ret, &class_map)
            })
            .unwrap_or(ret);
        let call_self_ty = if colon_call {
            callee.receiver.as_ref().and_then(|receiver| {
                receiver_class_type(self.model, receiver).or_else(|| Some(receiver.clone()))
            })
        } else {
            super::overload::call_operator_self_type(self.model, &callee.ty).or_else(|| {
                if matches!(callee.ty, LuaType::TableConst(_)) {
                    // For `setmetatable({}, { __call = function(self) ... end })`, the call
                    // operator's first param is the table itself, so `---@return self` should
                    // be replaced with that table type.
                    Some(callee.ty.clone())
                } else {
                    callee.receiver.as_ref().and_then(|receiver| {
                        receiver_class_type(self.model, receiver).or_else(|| Some(receiver.clone()))
                    })
                }
            })
        };
        let ret = call_self_ty
            .as_ref()
            .map(|self_ty| {
                let effective_self_ty = instantiate_self_type_with_call_generics(
                    self.model,
                    self_ty,
                    &callee_fun,
                    &bindings,
                );
                replace_self_type(&ret, &effective_self_ty)
            })
            .unwrap_or(ret);
        // Unbound generic params in the call result must not leak as bare TplRefs; degrade
        // them all to Unknown.
        let allowed_tpls: HashSet<GenericTplId> = HashSet::new();
        let ret = crate::semantic_model::type_eval::sanitize_unresolved_generics_with_model(
            self.model,
            &ret,
            &allowed_tpls,
        );
        let ret = self.expand_unpack_return(ret);
        let ret = self.expand_returned_variadic_function(ret, args, &callee_fun);
        Value::plain(ret)
    }

    /// Bind the actual callback call result to the original callback return generic (R / R...).
    /// Prefer locating TplRefs in the unsubstituted original return type; complex return
    /// types fall back to structural unify.
    fn bind_callback_return(
        &self,
        original_callback_ret: &LuaType,
        substituted_callback_ret: &LuaType,
        callback_ret: &LuaType,
        bindings: &mut unify::TplBindings,
    ) {
        match original_callback_ret {
            LuaType::TplRef(tpl) => {
                bindings.insert(tpl.get_tpl_id(), callback_ret.clone());
            }
            LuaType::Variadic(v) => {
                if let VariadicType::Base(LuaType::TplRef(tpl)) = v.as_ref() {
                    bindings.insert(tpl.get_tpl_id(), callback_ret.clone());
                } else if let VariadicType::Multi(types) = v.as_ref() {
                    // For `R1, R...` ("fixed first return + variadic tail return"), do not
                    // unify the actual callback's entire return list with the param return
                    // structure directly; first take the fixed slot from the actual callback's
                    // first return, then bind the remaining returns as the tail R.
                    let Some(LuaType::Variadic(tail_v)) = types.last() else {
                        let _ = unify_call_bindings(
                            self.model,
                            substituted_callback_ret,
                            callback_ret,
                            bindings,
                        );
                        return;
                    };
                    let VariadicType::Base(tail_base) = tail_v.as_ref() else {
                        let _ = unify_call_bindings(
                            self.model,
                            substituted_callback_ret,
                            callback_ret,
                            bindings,
                        );
                        return;
                    };
                    let callback_types: Vec<LuaType> = match callback_ret {
                        LuaType::Variadic(callback_variadic) => match callback_variadic.as_ref() {
                            VariadicType::Multi(callback_types) => callback_types.clone(),
                            VariadicType::Base(base) => vec![base.clone()],
                        },
                        other => vec![other.clone()],
                    };
                    let fixed_count = types.len() - 1;
                    let mut ok = true;
                    for (param, actual) in types[..fixed_count].iter().zip(callback_types.iter()) {
                        if !unify_call_bindings(self.model, param, actual, bindings) {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        let rest = if callback_types.len() > fixed_count {
                            if callback_types.len() == fixed_count + 1 {
                                callback_types[fixed_count].clone()
                            } else {
                                LuaType::Variadic(Arc::new(VariadicType::Multi(
                                    callback_types[fixed_count..].to_vec(),
                                )))
                            }
                        } else {
                            LuaType::Variadic(Arc::new(VariadicType::Base(LuaType::Nil)))
                        };
                        let _ = unify_call_bindings(self.model, tail_base, &rest, bindings);
                    }
                } else {
                    let _ = unify_call_bindings(
                        self.model,
                        substituted_callback_ret,
                        callback_ret,
                        bindings,
                    );
                }
            }
            _ => {
                let _ = unify_call_bindings(
                    self.model,
                    substituted_callback_ret,
                    callback_ret,
                    bindings,
                );
            }
        }
    }

    /// Field closures in table-literal args: fill the closure param environment using field
    /// signatures from the expected object type. Supports callback inference inside
    /// structural params like `fnA({ hook = function(obj) ... end })`.
    fn bind_table_literal_closure_params(
        &mut self,
        table_ty: &LuaType,
        expected_ty: &LuaType,
        bindings: &unify::TplBindings,
    ) {
        let LuaType::TableConst(table) = table_ty else {
            return;
        };
        let owner = SemanticId::member(table.file_id, table.value);
        let Some(tree) = self.model.syntax_tree_of(table.file_id) else {
            return;
        };
        let root = tree.get_red_root();
        for member_ref in self.model.members_of_owner(&owner) {
            let Some(facts) = self.model.file_facts_of(member_ref.file_id) else {
                continue;
            };
            let Some(member) = facts.member_by_id(&member_ref.id) else {
                continue;
            };
            let Some(value_syntax) = member.value_syntax else {
                continue;
            };
            let Some(node) = value_syntax.to_node_from_root(&root) else {
                continue;
            };
            let Some(closure) = LuaClosureExpr::cast(node) else {
                continue;
            };
            let Some(field_ty) = self.model.member_type(expected_ty, &member.key) else {
                continue;
            };
            let field_ty =
                crate::semantic_model::type_eval::expand_alias_generic(self.model, &field_ty);
            let Some(fun) = callback_fun_from_param(self.model, &field_ty) else {
                continue;
            };
            let closure_syntax = closure.get_syntax_id();
            for (param_idx, (_, param_ty)) in fun.get_params().iter().enumerate() {
                if let Some(param_ty) = param_ty {
                    let mut ty = unify::substitute(param_ty, bindings);
                    if !matches!(expected_ty, LuaType::Unknown | LuaType::Any) {
                        ty = replace_self_type(&ty, expected_ty);
                    }
                    self.closure_params.insert((closure_syntax, param_idx), ty);
                }
            }
        }
    }

    /// Evaluate a closure body's return type under the already-filled closure param
    /// environment. Used for higher-order generic closure-return back-inference
    /// (`fun(item: T): U` + `return item`).
    pub(crate) fn closure_return_type_with_env(&self, closure_syntax: LuaSyntaxId) -> LuaType {
        let _guard = ClosureReturnInferGuard::new(self.model, closure_syntax);
        let Some(tree) = self.model.syntax_tree() else {
            return LuaType::Unknown;
        };
        let root = tree.get_red_root();
        let Some(node) = closure_syntax.to_node_from_root(&root) else {
            return LuaType::Unknown;
        };
        let Some(closure) = LuaClosureExpr::cast(node) else {
            return LuaType::Unknown;
        };
        let return_exprs: Vec<LuaExpr> = closure
            .descendants::<LuaReturnStat>()
            .flat_map(|ret| ret.get_expr_list())
            .collect();
        if return_exprs.is_empty() {
            return LuaType::Unknown;
        }
        let mut types = Vec::new();
        for expr in return_exprs {
            let expr_syntax = expr.get_syntax_id();
            let mut code = Vec::new();
            compile(expr, self.model.file_id(), &mut code);
            code.push(Instr::Result);
            let mut vm = InferVm::new(self.model, &code);
            vm.closure_params = self.closure_params.clone();
            let mut ty = vm.run();
            // `---@as` / flow casts are attached to return expression nodes; flow queries can
            // give a more precise return type than bare VM (e.g. `{} --[[@as Promise<integer>]]`).
            let flow_ty =
                crate::semantic_model::flow::type_of_expr_with_cast(self.model, expr_syntax);
            if !matches!(flow_ty, LuaType::Unknown | LuaType::Any) && flow_ty != ty {
                ty = flow_ty;
            }
            if !matches!(ty, LuaType::Unknown | LuaType::Any) && !types.contains(&ty) {
                types.push(ty);
            }
        }
        if types.is_empty() {
            LuaType::Unknown
        } else if types.len() == 1 {
            types.pop().expect("len checked")
        } else {
            LuaType::from_vec(types)
        }
    }

    /// `std.Unpack<T>`: expand T (table literal/array/tuple) into multiple returns.
    fn expand_unpack_return(&self, ty: LuaType) -> LuaType {
        // For `T...` where T is a tuple/mapped tuple: expand the single tuple slot into
        // multi-return slots.
        if let LuaType::Variadic(variadic) = &ty {
            if let VariadicType::Base(base) = variadic.as_ref() {
                let slots: Option<Vec<LuaType>> = match base {
                    LuaType::Tuple(tuple) => Some(tuple.get_types().to_vec()),
                    LuaType::Object(object) => {
                        let mut indexed: Vec<(i64, LuaType)> = Vec::new();
                        for (key, ty) in object.get_fields() {
                            if let LuaMemberKey::Integer(i) = key {
                                indexed.push((*i, ty.clone()));
                            }
                        }
                        indexed.sort_by_key(|(i, _)| *i);
                        Some(indexed.into_iter().map(|(_, ty)| ty).collect())
                    }
                    _ => None,
                };
                if let Some(slots) = slots
                    && !slots.is_empty()
                {
                    return LuaType::Variadic(Arc::new(VariadicType::Multi(slots)));
                }
            }
        }
        let LuaType::Call(call) = &ty else {
            return ty;
        };
        if call.get_call_kind() != LuaAliasCallKind::Unpack {
            return ty;
        }
        let Some(operand) = call.get_operands().first() else {
            return ty;
        };
        match operand {
            LuaType::TableConst(table) => {
                let owner = SemanticId::member(table.file_id, table.value);
                let mut indexed: Vec<(i64, LuaType)> = Vec::new();
                for m in self.model.members_of_owner(&owner) {
                    if let Some(facts) = self.model.file_facts_of(m.file_id)
                        && let Some(member) = facts.member_by_id(&m.id)
                    {
                        if let LuaMemberKey::Integer(i) = &member.key {
                            let value = if let Some(value_syntax) = member.value_syntax {
                                self.model.type_of_expr(value_syntax)
                            } else {
                                self.model.type_of_member(&m.id).unwrap_or(LuaType::Unknown)
                            };
                            indexed.push((*i, value));
                        }
                    }
                }
                indexed.sort_by_key(|(i, _)| *i);
                LuaType::Variadic(Arc::new(VariadicType::Multi(
                    indexed.into_iter().map(|(_, v)| v).collect(),
                )))
            }
            LuaType::Array(array) => {
                let element = LuaType::from_vec(vec![array.get_base().clone(), LuaType::Nil]);
                LuaType::Variadic(Arc::new(VariadicType::Base(element)))
            }
            LuaType::Tuple(tuple) => {
                LuaType::Variadic(Arc::new(VariadicType::Multi(tuple.get_types().to_vec())))
            }
            LuaType::Object(object) => {
                let mut indexed: Vec<(i64, LuaType)> = Vec::new();
                for (key, ty) in object.get_fields() {
                    let LuaMemberKey::Name(name) = key else {
                        continue;
                    };
                    let s = name.as_str();
                    if s.starts_with('[')
                        && s.ends_with(']')
                        && let Ok(i) = s[1..s.len() - 1].parse::<i64>()
                    {
                        indexed.push((i, ty.clone()));
                    }
                }
                indexed.sort_by_key(|(i, _)| *i);
                LuaType::Variadic(Arc::new(VariadicType::Multi(
                    indexed.into_iter().map(|(_, v)| v).collect(),
                )))
            }
            _ => ty,
        }
    }

    /// After instantiating a higher-order generic returned function
    /// `fun(...: T...): R...`, if T is a tuple, expand it into fixed-length params; also try
    /// to preserve the argument function's original param names
    /// (`async_create(locaf)` -> `fun(a, b, c)`).
    fn expand_returned_variadic_function(
        &self,
        ret: LuaType,
        args: &[Value],
        callee_fun: &LuaFunctionType,
    ) -> LuaType {
        let LuaType::DocFunction(fun) = &ret else {
            return ret;
        };
        if fun.get_params().len() != 1 {
            return ret;
        }
        let (_, param_ty) = &fun.get_params()[0];
        let Some(LuaType::Variadic(variadic)) = param_ty else {
            return ret;
        };
        let Some(arg_fun) = args.iter().find_map(|arg| match &arg.ty {
            LuaType::DocFunction(f) => Some(f.as_ref().clone()),
            _ => arg
                .closure_syntax
                .and_then(|closure| self.model.type_of_signature(closure)),
        }) else {
            return ret;
        };
        let prefix_first = self.returned_variadic_is_prefix(callee_fun);
        let params = match variadic.as_ref() {
            VariadicType::Base(base) => match base {
                LuaType::Tuple(tuple) => {
                    if arg_fun.get_params().len() < tuple.get_types().len() {
                        return ret;
                    }
                    if prefix_first {
                        // For `fun(...:T..., cb: ...)`, `T...` is before `cb`: take prefix params.
                        let end = tuple.get_types().len();
                        arg_fun.get_params()[..end].to_vec()
                    } else {
                        // For `curry(foo, 'a')`'s `fun(_:T1..., _:T2...)` returning the T2 tail
                        // slot: take suffix params.
                        let start = arg_fun.get_params().len() - tuple.get_types().len();
                        arg_fun.get_params()[start..].to_vec()
                    }
                }
                LuaType::TplRef(_) | LuaType::Unknown | LuaType::Any | LuaType::Never => {
                    // Unbound variadic generic keeps `fun(...: T...)`; do not fix it to the
                    // argument function's params prematurely.
                    return ret;
                }
                _ => arg_fun.get_params().to_vec(),
            },
            VariadicType::Multi(_) => return ret,
        };
        let mut ret_ty = crate::semantic_model::type_eval::sanitize_unresolved_generics(
            fun.get_ret(),
            &HashSet::new(),
        );
        // After degradation, an unbound `R...` should not retain the "unknown variadic return"
        // shape; treat it as Unknown directly.
        if let LuaType::Variadic(variadic) = &ret_ty
            && matches!(variadic.as_ref(), VariadicType::Base(LuaType::Unknown))
        {
            ret_ty = LuaType::Unknown;
        }
        LuaType::DocFunction(Arc::new(LuaFunctionType::new(
            fun.get_async_state(),
            fun.is_colon_define(),
            false,
            params,
            ret_ty,
            None,
        )))
    }

    /// Determine whether a single variadic generic slot in the returned function is a prefix
    /// in the source function params (there are ordinary/callback params after it) or a
    /// suffix (last variadic slot, e.g. `curry`'s `T2...`). This decides whether to take the
    /// argument function's prefix or suffix params when expanding to fixed-length params.
    fn returned_variadic_is_prefix(&self, callee_fun: &LuaFunctionType) -> bool {
        let LuaType::DocFunction(return_fun) = callee_fun.get_ret() else {
            return false;
        };
        let Some((_, Some(LuaType::Variadic(return_v)))) = return_fun.get_params().first() else {
            return false;
        };
        let VariadicType::Base(return_base) = return_v.as_ref() else {
            return false;
        };
        let LuaType::TplRef(return_tpl) = return_base else {
            return false;
        };
        for (_, param_ty) in callee_fun.get_params() {
            let Some(LuaType::DocFunction(source_fun)) = param_ty else {
                continue;
            };
            let params = source_fun.get_params();
            for (idx, (_, ty)) in params.iter().enumerate() {
                let Some(LuaType::Variadic(v)) = ty else {
                    continue;
                };
                let VariadicType::Base(base) = v.as_ref() else {
                    continue;
                };
                let LuaType::TplRef(tpl) = base else {
                    continue;
                };
                if tpl.get_tpl_id() == return_tpl.get_tpl_id() {
                    return idx + 1 < params.len();
                }
            }
        }
        false
    }

    /// Call return: if the signature has `---@return_overload`, re-merge return slots after
    /// binding generics; otherwise directly substitute the candidate function's return type.
    fn substituted_call_return(
        &self,
        callee: &Value,
        callee_fun: &LuaFunctionType,
        bindings: &unify::TplBindings,
    ) -> LuaType {
        let owner_info = callee.owner.as_ref().and_then(|owner| match owner {
            SemanticId::Decl(key) => {
                let facts = self.model.file_facts_of(key.file_id)?;
                let decl = facts.decl_by_id(owner)?;
                Some((key.file_id, decl.value_expr_syntax?))
            }
            SemanticId::Member(key) => {
                let facts = self.model.file_facts_of(key.file_id)?;
                let member = facts.member_by_id(owner)?;
                Some((key.file_id, member.value_syntax?))
            }
            _ => None,
        });
        if let Some((file_id, value_syntax)) = owner_info {
            if let Some(facts) = self.model.file_facts_of(file_id)
                && let Some(signature) = facts.signature_by_closure(value_syntax)
                && let Some(docs) = signature.docs.as_ref()
                && !docs.return_overload_rows.is_empty()
            {
                let generic_params = docs.generic_params.clone();
                return self.return_overload_types_lua(
                    file_id,
                    docs,
                    &generic_params,
                    Some(bindings),
                );
            }
        }
        let ret = bind_signature_generics(callee_fun.get_ret(), callee_fun.get_generic_params());
        let ret = unify::substitute(&ret, bindings);
        let ret = crate::semantic_model::type_eval::expand_alias_generic(self.model, &ret);
        let ret = crate::semantic_model::type_eval::eval_conditionals(self.model, &ret);
        // `---@param ... T...` + `---@return T`: T is bound at the call site to the whole
        // variadic sequence (tuple). In "return that variadic sequence" semantics, the tuple
        // must expand into multiple return slots rather than being returned as one tuple value.
        if is_variadic_tuple_return(callee_fun, &ret)
            && let LuaType::Tuple(tuple) = &ret
            && !tuple.get_types().is_empty()
        {
            return LuaType::Variadic(Arc::new(VariadicType::Multi(tuple.get_types().to_vec())));
        }
        ret
    }

    /// Call candidates: signature docs (main signature + `---@overload`) take priority over
    /// an already projected `DocFunction`.
    fn callable_candidates(&self, callee: &Value) -> Vec<LuaFunctionType> {
        if let Some(owner) = &callee.owner
            && let Some(candidates) = self.signature_candidates(owner)
            && !candidates.is_empty()
        {
            return candidates;
        }

        if let LuaType::DocFunction(fun) = &callee.ty {
            return vec![fun.as_ref().clone()];
        }

        // The runtime table from `---@class Foo` + `local Foo = {}` is both a class table
        // and a callable object: calling `Foo()` directly should use the class's
        // `---@overload fun(...)` / `---@operator call`.
        if let Some(class_ty) = receiver_class_type(self.model, &callee.ty) {
            let mut visited = Vec::new();
            let candidates = self.expand_callable_types(&class_ty, &mut visited);
            if !candidates.is_empty() {
                return candidates;
            }
        }

        // Constructor class table produced by `meta("Class")`: `Class(...)` forwards to the
        // constructor method named by `---@[constructor("init")]` and uses class generic args
        // to infer the instance type.
        if let Some(attribute) = self.constructor_attribute_for_callee(&callee)
            && let Some(fun) = constructor_candidate(self.model, &callee.ty, &attribute)
        {
            return vec![fun];
        }

        // Recursive alias / union: expand and take function components.
        let mut visited = Vec::new();
        let candidates = self.expand_callable_types(&callee.ty, &mut visited);
        if candidates.is_empty() {
            // `setmetatable({}, { __call = ... })`: the table literal itself has no
            // signature, but the `__call` on its metatable makes it callable.
            if let Some(fun) = self.setmetatable_call_candidate(&callee) {
                return vec![fun];
            }
        }
        candidates
    }

    /// Resolve the `__call` candidate from a `setmetatable(t, mt)` initializer.
    ///
    /// Only handles callees whose declaration/table identity is directly created by
    /// `setmetatable(...)`; the candidate removes the first receiver param of `__call`
    /// (Lua's call operator automatically passes the called table).
    fn setmetatable_call_candidate(&self, callee: &Value) -> Option<LuaFunctionType> {
        let LuaType::TableConst(table) = &callee.ty else {
            return None;
        };
        Self::setmetatable_call_candidate_for_table(self.model, table)
    }

    /// Table identity -> `__call` metamethod candidate (shared by VM call and the
    /// `callable_functions` diagnostic).
    pub(crate) fn setmetatable_call_candidate_for_table(
        model: &SemanticModel,
        table: &InFiled<rowan::TextRange>,
    ) -> Option<LuaFunctionType> {
        let metatable_ty = crate::semantic_model::member::table_metatable_type(model, table)?;
        let call_info =
            model.member_info(&metatable_ty, &LuaMemberKey::Name(SmolStr::new("__call")))?;
        let call_file = call_info.file_id?;
        let call_member = call_info.id?;
        let facts = model.file_facts_of(call_file)?;
        let member = facts.member_by_id(&call_member)?;
        let value_syntax = member.value_syntax?;
        let mut fun = model.type_of_signature_in_file(call_file, value_syntax)?;
        // `---@return self` on a table-literal field may not be attached to a function
        // signature (salsa attaches the field doc to the member); for the `__call`
        // metamethod, when the return type is missing, treat it as the called table itself.
        if matches!(fun.get_ret(), LuaType::Unknown) {
            fun = LuaFunctionType::new(
                fun.get_async_state(),
                false,
                fun.is_variadic(),
                fun.get_params().to_vec(),
                LuaType::SelfInfer,
                Some(fun.get_generic_params().to_vec()),
            );
        }
        let mut params = fun.get_params().to_vec();
        // `__call`'s first param is the receiver (`self` or `_`), implicitly passed by the
        // call operator.
        if !params.is_empty() {
            params.remove(0);
        }
        Some(LuaFunctionType::new(
            fun.get_async_state(),
            false,
            fun.is_variadic(),
            params,
            fun.get_ret().clone(),
            Some(fun.get_generic_params().to_vec()),
        ))
    }

    /// Constructor attribute for a class-table callee: only named type definitions
    /// (Ref/Def) are recognized, and the runtime value factory call is traced back.
    fn constructor_attribute_for_callee(&self, callee: &Value) -> Option<ConstructorAttribute> {
        let id = match &callee.ty {
            LuaType::Ref(id) | LuaType::Def(id) => id,
            _ => return None,
        };
        let def = self.model.type_def_of(id)?;
        self.model.constructor_attribute_of_type(&def.id)
    }

    /// Look up function signatures from a declaration/member identity, building the main
    /// signature plus all `---@overload` candidates.
    fn signature_candidates(&self, owner: &SemanticId) -> Option<Vec<LuaFunctionType>> {
        let (file_id, value_syntax, overload_syntaxes) = match owner {
            SemanticId::Decl(key) => {
                let facts = self.model.file_facts_of(key.file_id)?;
                let decl = facts.decl_by_id(owner)?;
                let overloads = facts
                    .signature_by_closure(decl.value_expr_syntax?)
                    .and_then(|sig| sig.docs.as_ref())
                    .map(|docs| docs.overloads.clone())
                    .unwrap_or_default();
                (key.file_id, decl.value_expr_syntax?, overloads)
            }
            SemanticId::Member(key) => {
                let facts = self.model.file_facts_of(key.file_id)?;
                let member = facts.member_by_id(owner)?;
                let mut overload_syntaxes: Vec<LuaSyntaxId> = Vec::new();
                if let Some(signature) = facts.signature_by_closure(member.value_syntax?)
                    && let Some(docs) = signature.docs.as_ref()
                {
                    overload_syntaxes = docs.overloads.clone();
                }
                (key.file_id, member.value_syntax?, overload_syntaxes)
            }
            _ => return None,
        };
        let facts = self.model.file_facts_of(file_id)?;
        let signature = facts.signature_by_closure(value_syntax)?;
        // Runtime member methods without doc annotations: use signature projection (param
        // names + inferred return + is_colon_define). Decls (plain functions) still prefer
        // the existing `callee.ty` DocFunction (generic docs etc.) to avoid a no-doc body
        // projection overriding `---@type fun<T>...` declared types.
        if signature.docs.is_none() {
            if matches!(owner, SemanticId::Member(_)) {
                let fun = self
                    .model
                    .type_of_signature_in_file(file_id, value_syntax)?;
                return Some(vec![fun]);
            }
            return None;
        }
        let docs = signature.docs.as_ref()?;
        let generic_params = docs.generic_params.clone();

        let mut out = Vec::new();
        for overload_syntax in &overload_syntaxes {
            if let Some(fun) =
                self.doc_func_from_syntax(file_id, *overload_syntax, &generic_params, false)
            {
                out.push(fun);
            }
        }
        if let Some(main_fun) = self.main_signature_function(file_id, signature, &generic_params) {
            out.push(main_fun);
        }
        Some(out)
    }

    /// Build `GenericTpl`s with default/constraint metadata from `SalsaGenericParam`.
    /// Delegates uniformly to `SemanticModel`, sharing the same implementation as other
    /// signature-projection paths.
    fn generic_tpls_with_metadata(
        &self,
        file_id: FileId,
        params: &[SalsaGenericParam],
    ) -> Vec<GenericTpl> {
        self.model.generic_tpls_with_metadata(file_id, params)
    }

    /// Main signature (`---@param` + `---@return`), preserving variadic params and multi-returns.
    fn main_signature_function(
        &self,
        file_id: FileId,
        signature: &Signature,
        generic_params: &[SalsaGenericParam],
    ) -> Option<LuaFunctionType> {
        let docs = signature.docs.as_ref()?;
        let is_variadic = signature.is_variadic;
        let mut params = Vec::new();
        for name in &signature.param_names {
            let mut ty = docs
                .param_types
                .iter()
                .find(|(param_name, _)| param_name == name)
                .map(|(_, syntax)| {
                    self.doc_type_lua_with_generics(file_id, *syntax, generic_params)
                })
                .unwrap_or(LuaType::Any);
            if docs.nullable_params.iter().any(|n| n == name) && !ty.is_nullable() {
                ty = LuaType::Union(Arc::new(LuaUnionType::from_vec(vec![ty, LuaType::Nil])));
            }
            params.push((name.to_string(), Some(ty)));
        }

        let mut ret = self.return_overload_types_lua(file_id, docs, generic_params, None);
        let generic_tpls = self.generic_tpls_with_metadata(file_id, generic_params);
        ret = bind_signature_generics(&ret, &generic_tpls);
        let async_state = if docs.is_async {
            AsyncState::Async
        } else {
            AsyncState::None
        };
        Some(LuaFunctionType::new(
            async_state,
            signature.is_method,
            is_variadic,
            params,
            ret,
            Some(generic_tpls),
        ))
    }

    /// Multi-return projection: a single return keeps its type; multiple / variadic returns
    /// are packed as `Variadic::Multi`.
    fn return_types_lua(
        &self,
        file_id: FileId,
        returns: &[LuaSyntaxId],
        generic_params: &[SalsaGenericParam],
    ) -> LuaType {
        if returns.is_empty() {
            return LuaType::Unknown;
        }
        let mut types = returns
            .iter()
            .map(|syntax| self.doc_type_lua_with_generics(file_id, *syntax, generic_params))
            .collect::<Vec<_>>();
        if types.len() == 1 {
            return types.pop().unwrap_or(LuaType::Unknown);
        }
        LuaType::Variadic(Arc::new(VariadicType::Multi(types)))
    }

    /// `---@return_overload` multi-return slot merge (union across rows, missing slots
    /// filled with nil).
    fn return_overload_types_lua(
        &self,
        file_id: FileId,
        docs: &SignatureDoc,
        generic_params: &[SalsaGenericParam],
        bindings: Option<&unify::TplBindings>,
    ) -> LuaType {
        if docs.return_overload_rows.is_empty() {
            return self.return_types_lua(file_id, &docs.returns, generic_params);
        }

        let generic_tpls = self.generic_tpls_with_metadata(file_id, generic_params);

        fn flatten_row_types(
            types: &[LuaType],
            fixed: &mut Vec<LuaType>,
            tail: &mut Option<LuaType>,
        ) {
            for ty in types {
                match ty {
                    LuaType::Variadic(variadic) => match variadic.as_ref() {
                        VariadicType::Base(base) => {
                            if tail.is_none() {
                                *tail = Some(base.clone());
                            }
                        }
                        VariadicType::Multi(inner_types) => {
                            flatten_row_types(inner_types, fixed, tail);
                        }
                    },
                    _ => fixed.push(ty.clone()),
                }
            }
        }

        let mut rows: Vec<Vec<LuaType>> = Vec::new();
        let mut row_tails: Vec<Option<LuaType>> = Vec::new();
        let mut index = 0;
        for &len in &docs.return_overload_rows {
            let end = (index + len).min(docs.return_overloads.len());
            let raw_row: Vec<LuaType> = docs.return_overloads[index..end]
                .iter()
                .map(|(_, syntax)| {
                    let ty = self.doc_type_lua_with_generics(file_id, *syntax, generic_params);
                    let ty = bind_signature_generics(&ty, &generic_tpls);
                    if let Some(bindings) = bindings {
                        unify::substitute(&ty, bindings)
                    } else {
                        ty
                    }
                })
                .collect();
            let mut fixed = Vec::new();
            let mut tail = None;
            flatten_row_types(&raw_row, &mut fixed, &mut tail);
            rows.push(fixed);
            row_tails.push(tail);
            index = end;
        }

        let main_types: Vec<LuaType> = docs
            .returns
            .iter()
            .map(|syntax| {
                let ty = self.doc_type_lua_with_generics(file_id, *syntax, generic_params);
                let ty = bind_signature_generics(&ty, &generic_tpls);
                if let Some(bindings) = bindings {
                    unify::substitute(&ty, bindings)
                } else {
                    ty
                }
            })
            .collect();

        let use_prefix = !main_types.is_empty()
            && !main_types
                .iter()
                .any(|ty| matches!(ty, LuaType::Variadic(_)));
        let mut all_rows = rows.clone();
        let mut all_tails = row_tails.clone();
        if !use_prefix && !main_types.is_empty() {
            let mut fixed = Vec::new();
            let mut tail = None;
            flatten_row_types(&main_types, &mut fixed, &mut tail);
            all_rows.insert(0, fixed);
            all_tails.insert(0, tail);
        }
        let has_variadic = all_tails.iter().any(|tail| tail.is_some());

        let fixed_count = all_rows.iter().map(|row| row.len()).max().unwrap_or(0);
        let mut slots = Vec::new();
        for slot in 0..fixed_count {
            if use_prefix && slot < main_types.len() {
                slots.push(main_types[slot].clone());
                continue;
            }
            let mut components = if use_prefix {
                vec![LuaType::Nil]
            } else {
                Vec::new()
            };
            for (row, tail) in all_rows.iter().zip(all_tails.iter()) {
                let ty = if slot < row.len() {
                    row[slot].clone()
                } else if let Some(base) = tail {
                    base.clone()
                } else {
                    LuaType::Nil
                };
                match ty {
                    LuaType::Union(union) => {
                        for member in union.into_vec() {
                            if !components.contains(&member) {
                                components.push(member);
                            }
                        }
                    }
                    _ => {
                        if !components.contains(&ty) {
                            components.push(ty);
                        }
                    }
                }
            }
            if components.len() == 1 {
                slots.push(components.pop().expect("len checked"));
            } else {
                slots.push(LuaType::Union(Arc::new(LuaUnionType::from_vec(components))));
            }
        }

        if has_variadic {
            let mut tail_components = if use_prefix {
                vec![LuaType::Nil]
            } else {
                Vec::new()
            };
            let tail_rows: Vec<_> = if use_prefix {
                rows.iter().zip(row_tails.iter()).collect()
            } else {
                all_rows.iter().zip(all_tails.iter()).collect()
            };
            for (_, tail) in tail_rows {
                if let Some(base) = tail {
                    match base {
                        LuaType::Union(union) => {
                            for member in union.into_vec() {
                                if !tail_components.contains(&member) {
                                    tail_components.push(member);
                                }
                            }
                        }
                        _ => {
                            if !tail_components.contains(base) {
                                tail_components.push(base.clone());
                            }
                        }
                    }
                } else if !tail_components.contains(&LuaType::Nil) {
                    tail_components.push(LuaType::Nil);
                }
            }
            let tail_ty = if tail_components.len() == 1 {
                tail_components.pop().expect("len checked")
            } else {
                LuaType::Union(Arc::new(LuaUnionType::from_vec(tail_components)))
            };
            slots.push(LuaType::Variadic(Arc::new(VariadicType::Base(tail_ty))));
        }

        if slots.len() == 1 {
            slots.pop().expect("len checked")
        } else {
            LuaType::Variadic(Arc::new(VariadicType::Multi(slots)))
        }
    }

    /// `---@overload fun(...)` / `fun(...)` doc node -> function type.
    fn doc_func_from_syntax(
        &self,
        file_id: FileId,
        syntax: LuaSyntaxId,
        generic_params: &[SalsaGenericParam],
        is_method: bool,
    ) -> Option<LuaFunctionType> {
        let tree = self.model.syntax_tree_of(file_id)?;
        let root = tree.get_red_root();
        let node = syntax.to_node_from_root(&root)?;
        let doc_ty = LuaDocType::cast(node)?;
        match doc_ty {
            LuaDocType::Func(func) => {
                self.doc_func_from_ast(file_id, &func, generic_params, is_method)
            }
            _ => None,
        }
    }

    /// Build a function type from the `LuaDocType::Func` AST.
    fn doc_func_from_ast(
        &self,
        file_id: FileId,
        func: &emmylua_parser::LuaDocFuncType,
        outer_generics: &[SalsaGenericParam],
        is_method: bool,
    ) -> Option<LuaFunctionType> {
        // A function type's own generic declarations shadow same-named outer generics (when
        // `fun<T>(value: T): T` is returned inside an outer `---@generic T`, T is the returned
        // function's generic, not the outer T).
        let mut own_generics: Vec<SalsaGenericParam> = Vec::new();
        if let Some(decl_list) = func.get_generic_decl_list() {
            for decl in decl_list.get_generic_decl() {
                if let Some(token) = decl.get_name_token() {
                    own_generics.push(SalsaGenericParam::new(
                        SmolStr::new(token.get_name_text()),
                        decl.get_constraint_type().map(|t| t.get_syntax_id()),
                        decl.get_default_type().map(|t| t.get_syntax_id()),
                        decl.has_const_modifier(),
                        decl.is_variadic(),
                    ));
                }
            }
        }
        let has_own_generics = !own_generics.is_empty();
        let generic_params = if has_own_generics {
            own_generics
        } else {
            outer_generics.to_vec()
        };

        let mut params = Vec::new();
        let mut is_variadic = false;
        for param in func.get_params() {
            if param.is_dots() {
                is_variadic = true;
            }
            let name = param
                .get_name_token()
                .map(|token| token.get_name_text().to_string())
                .unwrap_or_else(|| {
                    if param.is_dots() {
                        "...".to_string()
                    } else {
                        String::new()
                    }
                });
            let ty = match param.get_type() {
                Some(doc_ty) => self.doc_type_lua_with_generics(
                    file_id,
                    doc_ty.get_syntax_id(),
                    &generic_params,
                ),
                None => LuaType::Unknown,
            };
            params.push((name, Some(ty)));
        }

        let ret = match func.get_return_type_list() {
            Some(list) => {
                let mut types = Vec::new();
                for ret in list.get_return_type_list() {
                    if let (_, Some(ret_type)) = ret.get_name_and_type() {
                        types.push(self.doc_type_lua_with_generics(
                            file_id,
                            ret_type.get_syntax_id(),
                            &generic_params,
                        ));
                    }
                }
                if types.is_empty() {
                    LuaType::Unknown
                } else if types.len() == 1 {
                    types.pop().unwrap_or(LuaType::Unknown)
                } else {
                    LuaType::Variadic(Arc::new(VariadicType::Multi(types)))
                }
            }
            None => LuaType::Unknown,
        };

        let mut fun = LuaFunctionType::new(
            if func.is_async() {
                AsyncState::Async
            } else {
                AsyncState::None
            },
            is_method,
            is_variadic,
            params,
            ret,
            Some(self.generic_tpls_with_metadata(file_id, &generic_params)),
        );
        if has_own_generics {
            fun = reassign_function_generics_to_func_ids(fun);
        }
        Some(fun)
    }

    /// Doc type projection: `T...` / `fun(...)` go through AST, everything else uses salsa
    /// projection.
    fn doc_type_lua_with_generics(
        &self,
        file_id: FileId,
        type_syntax: LuaSyntaxId,
        generic_params: &[SalsaGenericParam],
    ) -> LuaType {
        let Some(tree) = self.model.syntax_tree_of(file_id) else {
            return self
                .model
                .q()
                .doc_type_lua(file_id, type_syntax, generic_params);
        };
        let root = tree.get_red_root();
        let Some(node) = type_syntax.to_node_from_root(&root) else {
            return self
                .model
                .q()
                .doc_type_lua(file_id, type_syntax, generic_params);
        };
        let Some(doc_ty) = LuaDocType::cast(node) else {
            return self
                .model
                .q()
                .doc_type_lua(file_id, type_syntax, generic_params);
        };

        match doc_ty {
            LuaDocType::Variadic(variadic) => {
                let inner = match variadic.get_type() {
                    Some(inner) => self.doc_type_lua_with_generics(
                        file_id,
                        inner.get_syntax_id(),
                        generic_params,
                    ),
                    None => LuaType::Unknown,
                };
                LuaType::Variadic(Arc::new(VariadicType::Base(inner)))
            }
            LuaDocType::Func(func) => {
                match self.doc_func_from_ast(file_id, &func, generic_params, false) {
                    Some(fun) => LuaType::DocFunction(Arc::new(fun)),
                    None => LuaType::Unknown,
                }
            }
            LuaDocType::Literal(_)
            | LuaDocType::Object(_)
            | LuaDocType::Binary(_)
            | LuaDocType::IndexAccess(_)
            | LuaDocType::Mapped(_)
            | LuaDocType::Unary(_)
            | LuaDocType::Conditional(_) => self.model.doc_type_lua_rich_in(file_id, type_syntax),
            _ => self
                .model
                .q()
                .doc_type_lua(file_id, type_syntax, generic_params),
        }
    }

    /// Expand function candidates in named aliases / unions / intersections /
    /// `---@overload` / `---@operator call`.
    fn expand_callable_types(
        &self,
        ty: &LuaType,
        visited: &mut Vec<LuaTypeDeclId>,
    ) -> Vec<LuaFunctionType> {
        match ty {
            LuaType::DocFunction(fun) => vec![fun.as_ref().clone()],
            LuaType::Union(union) => union
                .into_vec()
                .iter()
                .flat_map(|component| self.expand_callable_types(component, visited))
                .collect(),
            LuaType::Intersection(intersection) => intersection
                .get_types()
                .iter()
                .flat_map(|component| self.expand_callable_types(component, visited))
                .collect(),
            LuaType::Ref(id) | LuaType::Def(id) => {
                if visited.contains(id) {
                    return Vec::new();
                }
                visited.push(id.clone());
                let mut out = Vec::new();
                if let Some(def) = self.model.type_def_of(id)
                    && let Some(target) = self.model.alias_target(&def)
                {
                    out = self.expand_callable_types(&target, visited);
                }
                if out.is_empty()
                    && let Some(def) = self.model.type_def_of(id)
                {
                    for syntax in &def.call_overloads {
                        if let Some(fun) = self.doc_func_from_syntax(
                            def.file_id,
                            *syntax,
                            &def.generic_params,
                            false,
                        ) {
                            out.push(fun);
                        }
                    }
                    if let Some(facts) = self.model.file_facts_of(def.file_id)
                        && let Some(op) = facts.operator_of(&def.id, "call")
                    {
                        let params = op
                            .params
                            .iter()
                            .map(|syntax| {
                                (
                                    String::new(),
                                    Some(self.model.doc_type_lua_rich_in(def.file_id, *syntax)),
                                )
                            })
                            .collect();
                        out.push(LuaFunctionType::new(
                            AsyncState::None,
                            false,
                            false,
                            params,
                            self.model.doc_type_lua_rich_in(def.file_id, op.returns),
                            None,
                        ));
                    }
                }
                visited.pop();
                out
            }
            LuaType::Generic(generic) => {
                let base_id = generic.get_base_type_id();
                if visited.contains(&base_id) {
                    return Vec::new();
                }
                visited.push(base_id.clone());
                let mut out = Vec::new();
                if let Some(def) = self.model.type_def_of(&base_id)
                    && def.kind == TypeDefKind::Alias
                    && let Some(target) = self.model.alias_target(&def)
                {
                    let bindings: unify::TplBindings = generic
                        .get_params()
                        .iter()
                        .enumerate()
                        .map(|(index, ty)| (GenericTplId::Type(index as u32), ty.clone()))
                        .collect();
                    let instantiated = unify::substitute(&target, &bindings);
                    out = self.expand_callable_types(&instantiated, visited);
                }
                if out.is_empty()
                    && let Some(def) = self.model.type_def_of(&base_id)
                {
                    for syntax in &def.call_overloads {
                        if let Some(fun) = self.doc_func_from_syntax(
                            def.file_id,
                            *syntax,
                            &def.generic_params,
                            false,
                        ) {
                            out.push(fun);
                        }
                    }
                    if let Some(facts) = self.model.file_facts_of(def.file_id)
                        && let Some(op) = facts.operator_of(&def.id, "call")
                    {
                        let params = op
                            .params
                            .iter()
                            .map(|syntax| {
                                (
                                    String::new(),
                                    Some(self.model.doc_type_lua_rich_in(def.file_id, *syntax)),
                                )
                            })
                            .collect();
                        out.push(LuaFunctionType::new(
                            AsyncState::None,
                            false,
                            false,
                            params,
                            self.model.doc_type_lua_rich_in(def.file_id, op.returns),
                            None,
                        ));
                    }
                }
                visited.pop();
                out
            }
            _ => Vec::new(),
        }
    }

    /// Candidate selection: among candidates where all params unify successfully, take the
    /// highest score (literal/function structural exact matches preferred).
    fn select_callable(
        &self,
        candidates: &[LuaFunctionType],
        args: &[Value],
        colon_call: bool,
        receiver: Option<&LuaType>,
    ) -> Option<(LuaFunctionType, unify::TplBindings)> {
        let call_args: Vec<super::overload::CallArg> = args
            .iter()
            .map(|arg| super::overload::CallArg {
                ty: arg.ty.clone(),
                closure_syntax: arg.closure_syntax,
            })
            .collect();
        super::overload::select_callable(self.model, candidates, &call_args, colon_call, receiver)
    }

    /// Fallback selection when no candidate fully matches: allow a plain `... T` to infer
    /// generics from the first actual arg.
    fn select_callable_partial(
        &self,
        candidates: &[LuaFunctionType],
        args: &[Value],
        colon_call: bool,
        receiver: Option<&LuaType>,
    ) -> Option<(LuaFunctionType, unify::TplBindings)> {
        let call_args: Vec<super::overload::CallArg> = args
            .iter()
            .map(|arg| super::overload::CallArg {
                ty: arg.ty.clone(),
                closure_syntax: arg.closure_syntax,
            })
            .collect();
        super::overload::select_callable_partial(
            self.model, candidates, &call_args, colon_call, receiver,
        )
    }
}

/// Whether an owner identity directly corresponds to a global class table name (the
/// declaration name of a `Decl`/`Name`).
fn owner_is_class_table_name(model: &SemanticModel, owner: &SemanticId, class_name: &str) -> bool {
    match owner {
        SemanticId::Name(name) => name.as_str() == class_name,
        SemanticId::Decl(key) => model
            .file_facts_of(key.file_id)
            .and_then(|facts| facts.decl_by_id(&SemanticId::Decl(key.clone())))
            .is_some_and(|decl| decl.name.as_str() == class_name),
        _ => false,
    }
}

/// Generate a "class-call candidate" function type for a constructor attribute.
///
/// The candidate's first param is fixed to the class table itself (acting as the
/// constructor method's implicit `self`); later params come from the constructor method's
/// user params. Function-level generics directly use the class's generic params, so
/// `Class("x")` can back-infer instance generics like `Class<string>`. The return type is
/// generated by attribute mode: `Doc` uses the constructor method's doc return, other modes
/// return an instance type with class generic args.
fn constructor_candidate(
    model: &SemanticModel,
    class_ty: &LuaType,
    attribute: &ConstructorAttribute,
) -> Option<LuaFunctionType> {
    let (LuaType::Ref(id) | LuaType::Def(id)) = class_ty else {
        return None;
    };
    let def = model.type_def_of(id)?;
    let key = LuaMemberKey::Name(attribute.name.clone());
    let member_info = model
        .member_infos_with_key(class_ty, &key)
        .into_iter()
        .next()
        .or_else(|| {
            // Methods on a global class table (`function ClassB:init`) hang on
            // `Name("ClassB")`, while member-type collection only covers TypeDef/runtime Decl
            // identities; query the Name owner directly here.
            model
                .members_of_owner(&SemanticId::name(def.name.clone()))
                .into_iter()
                .find(|member| member.name == attribute.name)
                .map(|member| crate::semantic_model::member::MemberInfo {
                    key: key.clone(),
                    typ: model.type_of_member(&member.id).unwrap_or(LuaType::Unknown),
                    id: Some(member.id.clone()),
                    file_id: Some(member.file_id),
                    is_method: true,
                })
        })?;
    let member_file = member_info.file_id?;
    let member_id = member_info.id?;
    let facts = model.file_facts_of(member_file)?;
    let member = facts.member_by_id(&member_id)?;
    let value_syntax = member.value_syntax?;
    let method_fun = model.type_of_signature_in_file(member_file, value_syntax)?;

    let class_generics = model.generic_tpls_with_metadata(def.file_id, &def.generic_params);

    let mut params = Vec::new();
    for (name, ty) in method_fun.get_params() {
        if name == "self" || matches!(&ty, Some(LuaType::SelfInfer)) {
            continue;
        }
        let ty = ty
            .as_ref()
            .map(|ty| bind_signature_generics(ty, &class_generics));
        params.push((name.clone(), ty));
    }

    let ret = match attribute.return_mode {
        ConstructorReturnMode::Doc => {
            bind_signature_generics(method_fun.get_ret(), &class_generics)
        }
        ConstructorReturnMode::SelfType | ConstructorReturnMode::Default => {
            constructor_return_instance(class_ty, &class_generics)
        }
    };

    Some(LuaFunctionType::new(
        AsyncState::None,
        false,
        method_fun.is_variadic(),
        params,
        ret,
        Some(class_generics),
    ))
}

fn constructor_return_instance(class_ty: &LuaType, generics: &[GenericTpl]) -> LuaType {
    if generics.is_empty() {
        return class_ty.clone();
    }
    let (LuaType::Ref(id) | LuaType::Def(id)) = class_ty else {
        return class_ty.clone();
    };
    let params = generics
        .iter()
        .map(|generic| LuaType::TplRef(Arc::new(generic.clone())))
        .collect();
    LuaType::Generic(Arc::new(LuaGenericType::new(id.clone(), params)))
}

pub(crate) fn widen_const(ty: &LuaType) -> LuaType {
    match ty {
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => LuaType::String,
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => LuaType::Integer,
        _ => ty.clone(),
    }
}

/// Used when collecting args for variadic slots: `... T...` treats each arg as a type slot,
/// so boolean literals must widen to `boolean` (ordinary generics keep literals for
/// return_overload-related flows).
pub(crate) fn widen_variadic_const(ty: &LuaType) -> LuaType {
    match ty {
        LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => LuaType::Boolean,
        _ => widen_const(ty),
    }
}

/// Closures in generic param positions should use a "shallow signature": unannotated params
/// remain untyped (rather than `unknown`), and undeclared returns become `any`.
///
/// This is only for binding a closure literal as a **generic value** to `T` (e.g.
/// `---@generic T: Procedure; ---@param a T`). If the closure is in a callback param
/// (`fun(...: T...)`), the existing closure param/return inference is used instead.
pub(crate) fn shallow_closure_signature(
    model: &SemanticModel,
    closure_syntax: LuaSyntaxId,
) -> LuaType {
    let Some(fun) = model.type_of_signature(closure_syntax) else {
        return LuaType::Unknown;
    };
    let Some(facts) = model.file_facts() else {
        return LuaType::DocFunction(Arc::new(fun));
    };
    let has_explicit_return = facts
        .signature_by_closure(closure_syntax)
        .and_then(|sig| sig.docs.as_ref())
        .is_some_and(|docs| !docs.returns.is_empty() || !docs.return_overloads.is_empty());
    let params = fun
        .get_params()
        .iter()
        .map(|(name, ty)| {
            let ty = match ty {
                Some(LuaType::Unknown) => None,
                other => other.clone(),
            };
            (name.clone(), ty)
        })
        .collect();
    let ret = if has_explicit_return {
        fun.get_ret().clone()
    } else {
        LuaType::Any
    };
    LuaType::DocFunction(Arc::new(LuaFunctionType::new(
        fun.get_async_state(),
        fun.is_colon_define(),
        fun.is_variadic(),
        params,
        ret,
        Some(fun.get_generic_params().to_vec()),
    )))
}

/// Table-literal receiver -> the named type from its `---@class` annotation (e.g. stdlib
/// global `table` -> `tablelib`).
fn receiver_class_type(model: &SemanticModel, ty: &LuaType) -> Option<LuaType> {
    let LuaType::TableConst(table) = ty else {
        return None;
    };
    let facts = model.file_facts_of(table.file_id)?;
    let decl = facts
        .decls
        .iter()
        .find(|d| d.value_expr_syntax.map(|s| s.get_range()) == Some(table.value))?;
    let def = facts
        .type_defs
        .iter()
        .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl.owner_syntax)?;
    Some(match def.visibility {
        TypeVisibility::Public => LuaType::Ref(LuaTypeDeclId::global(&def.full_name)),
        _ => LuaType::Ref(LuaTypeDeclId::file(def.file_id, &def.full_name)),
    })
}

/// When constructing a `self` return type, replace the class reference with an instance that
/// carries generic args.
///
/// For example, after calling the constructor overload
/// `GenericClass<T>`'s `fun(t: T): self` with `GenericClass(ext)`, `self` should be
/// `GenericClass<ExtendedClass>`; for a no-arg constructor `A<T>`, `fun(): self` is
/// `A<unknown>`.
fn instantiate_self_type_with_call_generics(
    model: &SemanticModel,
    self_ty: &LuaType,
    fun: &LuaFunctionType,
    bindings: &unify::TplBindings,
) -> LuaType {
    let (LuaType::Ref(id) | LuaType::Def(id)) = self_ty else {
        return self_ty.clone();
    };
    let Some(def) = model.type_def_of(id) else {
        return self_ty.clone();
    };
    if def.generic_params.is_empty() {
        return self_ty.clone();
    }
    let fun_bindings: HashMap<&str, &LuaType> = bindings
        .iter()
        .filter_map(|(tpl_id, ty)| {
            fun.get_generic_params()
                .iter()
                .find(|param| param.get_tpl_id() == *tpl_id)
                .map(|param| (param.get_name(), ty))
        })
        .collect();
    let params: Vec<LuaType> = def
        .generic_params
        .iter()
        .map(|param| {
            if let Some(bound) = fun_bindings.get(param.name.as_str()) {
                (*bound).clone()
            } else if let Some(default) = param.default {
                model.doc_type_lua_in(def.file_id, default, &def.generic_params)
            } else if let Some(constraint) = param.constraint {
                model.doc_type_lua_in(def.file_id, constraint, &def.generic_params)
            } else {
                LuaType::Unknown
            }
        })
        .collect();
    LuaType::Generic(Arc::new(LuaGenericType::new(id.clone(), params)))
}

/// Replace `self` placeholders in a type with the actual receiver type (for method-call
/// returns/params).
pub(crate) fn replace_self_type(ty: &LuaType, self_ty: &LuaType) -> LuaType {
    use LuaType::*;
    match ty {
        SelfInfer => self_ty.clone(),
        Array(array) => Array(Arc::new(LuaArrayType::from_base_type(replace_self_type(
            array.get_base(),
            self_ty,
        )))),
        Tuple(tuple) => Tuple(Arc::new(LuaTupleType::new(
            tuple
                .get_types()
                .iter()
                .map(|t| replace_self_type(t, self_ty))
                .collect(),
            tuple.status,
        ))),
        DocFunction(fun) => DocFunction(Arc::new(LuaFunctionType::new(
            fun.get_async_state(),
            fun.is_colon_define(),
            fun.is_variadic(),
            fun.get_params()
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        ty.as_ref().map(|t| replace_self_type(t, self_ty)),
                    )
                })
                .collect(),
            replace_self_type(fun.get_ret(), self_ty),
            Some(fun.get_generic_params().to_vec()),
        ))),
        Object(object) => Object(Arc::new(LuaObjectType::new_with_fields(
            object
                .get_fields()
                .iter()
                .map(|(key, ty)| (key.clone(), replace_self_type(ty, self_ty)))
                .collect(),
            object
                .get_index_access()
                .iter()
                .map(|(key, ty)| {
                    (
                        replace_self_type(key, self_ty),
                        replace_self_type(ty, self_ty),
                    )
                })
                .collect(),
        ))),
        Union(union) => Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|t| replace_self_type(t, self_ty))
                .collect(),
        ))),
        Intersection(intersection) => Intersection(Arc::new(crate::LuaIntersectionType::new(
            intersection
                .get_types()
                .iter()
                .map(|t| replace_self_type(t, self_ty))
                .collect(),
        ))),
        Generic(generic) => Generic(Arc::new(LuaGenericType::new(
            generic.get_base_type_id(),
            generic
                .get_params()
                .iter()
                .map(|t| replace_self_type(t, self_ty))
                .collect(),
        ))),
        TableGeneric(generic) => TableGeneric(Arc::new(
            generic
                .iter()
                .map(|t| replace_self_type(t, self_ty))
                .collect(),
        )),
        Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
            VariadicType::Base(base) => VariadicType::Base(replace_self_type(base, self_ty)),
            VariadicType::Multi(types) => VariadicType::Multi(
                types
                    .iter()
                    .map(|t| replace_self_type(t, self_ty))
                    .collect(),
            ),
        })),
        Call(call) => Call(Arc::new(crate::LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|t| replace_self_type(t, self_ty))
                .collect(),
        ))),
        Instance(instance) => Instance(Arc::new(crate::LuaInstanceType::new(
            replace_self_type(instance.get_base(), self_ty),
            instance.get_range().clone(),
        ))),
        TypeGuard(guard) => TypeGuard(Arc::new(replace_self_type(guard, self_ty))),
        Conditional(conditional) => Conditional(Arc::new(crate::LuaConditionalType::new(
            replace_self_type(conditional.get_checked_type(), self_ty),
            replace_self_type(conditional.get_extends_type(), self_ty),
            replace_self_type(conditional.get_true_type(), self_ty),
            replace_self_type(conditional.get_false_type(), self_ty),
            conditional.get_infer_params().to_vec(),
            conditional.has_new,
        ))),
        Mapped(mapped) => Mapped(Arc::new(crate::LuaMappedType::new(
            mapped.param.clone(),
            replace_self_type(&mapped.value, self_ty),
            mapped.is_readonly,
            mapped.is_optional,
        ))),
        MultiLineUnion(union) => MultiLineUnion(Arc::new(crate::LuaMultiLineUnion::new(
            union
                .get_unions()
                .iter()
                .map(|(ty, desc)| (replace_self_type(ty, self_ty), desc.clone()))
                .collect(),
        ))),
        _ => ty.clone(),
    }
}

fn resolve_function_ret_defaults(ty: &LuaType, fun: &LuaFunctionType) -> LuaType {
    match ty {
        LuaType::TplRef(tpl) => {
            for param in fun.get_generic_params() {
                if param.get_tpl_id() == tpl.get_tpl_id() {
                    if let Some(default) = param.get_default_type() {
                        return default.clone();
                    }
                    if let Some(constraint) = param.get_constraint() {
                        return constraint.clone();
                    }
                }
            }
            ty.clone()
        }
        LuaType::Union(union) => LuaType::Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|t| resolve_function_ret_defaults(t, fun))
                .collect(),
        ))),
        LuaType::Variadic(variadic) => {
            let resolved = match variadic.as_ref() {
                VariadicType::Base(base) => {
                    VariadicType::Base(resolve_function_ret_defaults(base, fun))
                }
                VariadicType::Multi(types) => VariadicType::Multi(
                    types
                        .iter()
                        .map(|t| resolve_function_ret_defaults(t, fun))
                        .collect(),
                ),
            };
            LuaType::Variadic(Arc::new(resolved))
        }
        _ => ty.clone(),
    }
}

fn primitive_accepts_const(param: &LuaType, arg: &LuaType) -> bool {
    matches!(
        (param, arg),
        (
            LuaType::Integer,
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
        ) | (
            LuaType::Number,
            LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) | LuaType::FloatConst(_)
        ) | (
            LuaType::String,
            LuaType::StringConst(_) | LuaType::DocStringConst(_)
        ) | (
            LuaType::Boolean,
            LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_)
        )
    )
}

/// Try to project a generic arg (subclass) to a target base class, for inherited-generic
/// unify like `Subject<T> -> Observable<T>`.
///
/// First read the full parent-type args from the class declaration's `---@class` syntax
/// (e.g. `C_StringHolderWith<T> : Holder<string>`); if that syntax is unavailable, fall back
/// to the old behavior of filling parent generic args by subclass generic-arg order.
fn super_generic_type(
    model: &SemanticModel,
    arg_ty: &LuaType,
    target_base: &LuaTypeDeclId,
) -> Option<LuaType> {
    use LuaType::*;
    let (arg_def, arg_params): (TypeDef, Option<Vec<LuaType>>) = match arg_ty {
        Generic(arg_generic) => {
            let def = model.type_def_of(&arg_generic.get_base_type_id())?;
            (def, Some(arg_generic.get_params().to_vec()))
        }
        Ref(id) | Def(id) => (model.type_def_of(id)?, None),
        _ => return None,
    };

    // Read full parent-type args from the `---@class` doc tag in the source.
    if let Some(tree) = model.syntax_tree_of(arg_def.file_id) {
        let root = tree.get_red_root();
        for node in root.descendants() {
            let Some(tag) = LuaDocTag::cast(node) else {
                continue;
            };
            let LuaDocTag::Class(class_tag) = tag else {
                continue;
            };
            let Some(name_token) = class_tag.get_name_token() else {
                continue;
            };
            if name_token.get_name_text() != arg_def.name.as_str() {
                continue;
            }
            let Some(supers) = class_tag.get_supers() else {
                continue;
            };
            for super_doc in supers.get_types() {
                let (super_name, super_args) = match &super_doc {
                    LuaDocType::Name(name_ty) => (name_ty.get_name_text()?, Vec::new()),
                    LuaDocType::Generic(generic_ty) => (
                        generic_ty.get_name_type()?.get_name_text()?,
                        generic_ty
                            .get_generic_types()
                            .map(|list| list.get_types().collect::<Vec<_>>())
                            .unwrap_or_default(),
                    ),
                    _ => continue,
                };
                let super_id = LuaTypeDeclId::global(&super_name);
                let super_def = model.resolve_type_def(super_name.as_str())?;
                let super_ty = if super_args.is_empty() || super_def.generic_params.is_empty() {
                    Ref(super_id.clone())
                } else {
                    let mut params = Vec::new();
                    let mut tpl_bindings = unify::TplBindings::new();
                    if let Some(arg_params) = &arg_params {
                        for (index, value) in arg_params.iter().enumerate() {
                            tpl_bindings.insert(GenericTplId::Type(index as u32), value.clone());
                        }
                    }
                    for arg_doc in super_args {
                        let mut ty = model.doc_type_lua_in(
                            arg_def.file_id,
                            arg_doc.get_syntax_id(),
                            &arg_def.generic_params,
                        );
                        if let Some(arg_params) = &arg_params {
                            let name_bindings: HashMap<std::string::String, LuaType> = arg_def
                                .generic_params
                                .iter()
                                .enumerate()
                                .filter_map(|(index, param)| {
                                    arg_params
                                        .get(index)
                                        .map(|value| (param.name.to_string(), value.clone()))
                                })
                                .collect();
                            ty = unify::substitute(&ty, &tpl_bindings);
                            ty = crate::semantic_model::type_eval::substitute_named_refs(
                                &ty,
                                &name_bindings,
                            );
                        }
                        params.push(ty);
                    }
                    Generic(Arc::new(LuaGenericType::new(super_id.clone(), params)))
                };
                if super_id.get_simple_name() == target_base.get_simple_name() {
                    return Some(super_ty);
                }
                // The parent may still not be the target base class
                // (`C_StringHolderExt : C_StringHolder`); continue projecting up the chain.
                if let Some(recursive) = super_generic_type(model, &super_ty, target_base) {
                    return Some(recursive);
                }
            }
        }
    }

    // Fallback: without exact parent-type args, fill by subclass arg order (old behavior).
    let arg_params = arg_params?;
    for super_name in &arg_def.super_names {
        let super_id = LuaTypeDeclId::global(super_name);
        if &super_id != target_base && super_id.get_simple_name() != target_base.get_simple_name() {
            continue;
        }
        let super_def = model.resolve_type_def(super_name)?;
        if super_def.generic_params.is_empty() {
            return Some(Ref(super_id));
        }
        if super_def.generic_params.len() == arg_params.len() {
            return Some(Generic(Arc::new(LuaGenericType::new(
                super_id,
                arg_params.clone(),
            ))));
        }
    }
    None
}

fn type_contains_any_tpl_id(ty: &LuaType, ids: &HashSet<GenericTplId>) -> bool {
    ty.any_nested_type(|t| matches!(t, LuaType::TplRef(tpl) if ids.contains(&tpl.get_tpl_id())))
}

pub(crate) fn unify_call_bindings(
    model: &SemanticModel,
    param: &LuaType,
    arg: &LuaType,
    bindings: &mut unify::TplBindings,
) -> bool {
    use LuaType::*;
    match param {
        Unknown | Any => true,
        TplRef(tpl) => {
            if let TplRef(arg_tpl) = arg
                && arg_tpl.get_tpl_id() == tpl.get_tpl_id()
            {
                return true;
            }
            let tpl_id = tpl.get_tpl_id();
            if let Some(existing) = bindings.get(&tpl_id).cloned() {
                if existing == *arg {
                    return true;
                }
                // An already-inferred generic value may accept a more specific actual arg
                // (e.g. param is `function`, actual is `fun(): integer`; or param is a callback
                // structure and actual is a more detailed closure). This lets higher-order calls
                // like `apply(run, cb)` both infer from f's shape and check later args.
                if matches!(arg, Unknown | Any | Never) {
                    return true;
                }
                if model.type_check_subtype(arg, &existing) {
                    return true;
                }
                // The already-bound structure may still contain unfilled nested generics (e.g.
                // `fun(x: integer): T`); do one structural unify with the current arg to fill
                // them without changing the existing outer binding.
                if unify_call_bindings(model, &existing, arg, bindings) {
                    return true;
                }
                return false;
            }
            // `---@generic const T` or constrained generics (like `K extends keyof T`) keep
            // literals; ordinary generics widen per old behavior.
            let arg = if tpl.is_const() || tpl.get_constraint().is_some() {
                arg.clone()
            } else {
                widen_const(arg)
            };
            if !unify::unify_bindings(param, &arg, bindings) {
                return false;
            }
            if let Some(constraint) = tpl.get_constraint()
                && !matches!(constraint, Unknown | Call(_))
                && !model.type_check(&arg, constraint)
            {
                return false;
            }
            true
        }
        Variadic(param_variadic) => match param_variadic.as_ref() {
            VariadicType::Base(base) => match arg {
                Variadic(arg_variadic) => match arg_variadic.as_ref() {
                    VariadicType::Base(arg_base) => {
                        unify_call_bindings(model, base, arg_base, bindings)
                    }
                    VariadicType::Multi(arg_types) => {
                        if matches!(base, TplRef(_)) {
                            // `R...` needs the entire multi-return bound to generic R, not
                            // element by element.
                            unify_call_bindings(model, base, arg, bindings)
                        } else {
                            arg_types
                                .iter()
                                .all(|a| unify_call_bindings(model, base, a, bindings))
                        }
                    }
                },
                Unknown | Any => true,
                _ => unify_call_bindings(model, base, arg, bindings),
            },
            VariadicType::Multi(param_types) => match arg {
                Variadic(arg_variadic) => match arg_variadic.as_ref() {
                    VariadicType::Multi(arg_types) if param_types.len() == arg_types.len() => {
                        for (p, a) in param_types.iter().zip(arg_types.iter()) {
                            if !unify_call_bindings(model, p, a, bindings) {
                                return false;
                            }
                        }
                        true
                    }
                    _ => false,
                },
                Unknown | Any => true,
                _ if param_types.len() == 1 => {
                    unify_call_bindings(model, &param_types[0], arg, bindings)
                }
                // A `R1, R...` param return is compatible with an actual function that returns
                // only `R1`: the trailing variadic return may be empty.
                _ if matches!(
                    param_types.last(),
                    Some(Variadic(v)) if matches!(v.as_ref(), VariadicType::Base(_))
                ) && param_types.len() == 2 =>
                {
                    unify_call_bindings(model, &param_types[0], arg, bindings)
                }
                _ => false,
            },
        },
        Generic(param_generic) => {
            if let Generic(arg_generic) = arg {
                if param_generic.get_base_type_id() == arg_generic.get_base_type_id()
                    && param_generic.get_params().len() == arg_generic.get_params().len()
                {
                    return param_generic
                        .get_params()
                        .iter()
                        .zip(arg_generic.get_params())
                        .all(|(p, a)| unify_call_bindings(model, p, a, bindings));
                }
            }
            // Generic alias args (`A_StringHolder -> Holder<string>`) must be expanded to the
            // underlying structure before unifying/back-inferring with `Holder<T>` params.
            // `expand_alias_generic` only expands `Generic`-form aliases, so additional
            // bare-named alias expansion is added here.
            let expanded_arg = match arg {
                Ref(id) | Def(id) => model
                    .type_def_of(id)
                    .filter(|def| def.kind == TypeDefKind::Alias)
                    .and_then(|def| model.alias_target(&def))
                    .unwrap_or_else(|| arg.clone()),
                other => other.clone(),
            };
            let expanded_arg =
                crate::semantic_model::type_eval::expand_alias_generic(model, &expanded_arg);
            if &expanded_arg != arg {
                return unify_call_bindings(model, param, &expanded_arg, bindings);
            }
            // Subclass args (`C_StringHolder`, `C_StringHolderWith<table>`) may project onto
            // parent generics (`Holder<T>`), enabling `T` inference from `@param v Holder<T>`.
            if let Some(super_ty) =
                super_generic_type(model, arg, &param_generic.get_base_type_id())
            {
                return unify_call_bindings(model, param, &super_ty, bindings);
            }
            unify::unify_bindings(param, arg, bindings)
        }
        Table | TableConst(_) | TableGeneric(_) => true,
        Object(_param_object) => {
            if let Object(_arg_object) = arg {
                unify::unify_bindings(param, arg, bindings)
            } else {
                false
            }
        }
        DocFunction(param_fun) => {
            if let DocFunction(arg_fun) = arg {
                let param_params = param_fun.get_params();
                let arg_params = arg_fun.get_params();
                let mut i = 0;
                let mut arg_index = 0;
                while i < param_params.len() {
                    let Some((param_name, param_ty)) =
                        param_params.get(i).map(|p| (p.0.as_str(), p.1.clone()))
                    else {
                        break;
                    };
                    let Some(param_ty) = param_ty else {
                        i += 1;
                        continue;
                    };
                    if matches!(param_ty, Unknown | Any) {
                        i += 1;
                        continue;
                    }
                    // Param is a variadic slot (`fun(...: T..., cb: ...)`): the variadic only
                    // consumes the actual function's params that come before the later ordinary
                    // params; remaining ordinary params continue matching.
                    if param_name == "..." || matches!(param_ty, Variadic(_)) {
                        let trailing = param_params[i + 1..]
                            .iter()
                            .filter(|(n, ty)| n != "..." && !matches!(ty, Some(Variadic(_))))
                            .count();
                        let consume = if let Variadic(variadic) = &param_ty
                            && let VariadicType::Base(base) = variadic.as_ref()
                            && let TplRef(tpl) = base
                            && let Some(bound) = bindings.get(&tpl.get_tpl_id())
                        {
                            match bound {
                                Tuple(tuple) => tuple.get_types().len(),
                                Variadic(v) => match v.as_ref() {
                                    VariadicType::Multi(types) => types.len(),
                                    VariadicType::Base(_) => 1,
                                },
                                _ => 1,
                            }
                        } else {
                            arg_params
                                .len()
                                .saturating_sub(arg_index)
                                .saturating_sub(trailing)
                        };
                        let consume = consume.min(arg_params.len().saturating_sub(arg_index));
                        let remaining: Vec<LuaType> = arg_params[arg_index..arg_index + consume]
                            .iter()
                            .filter_map(|p| p.1.clone())
                            .collect();
                        if let Variadic(variadic) = &param_ty
                            && let VariadicType::Base(base) = variadic.as_ref()
                            && let TplRef(tpl) = base
                            && !bindings.contains_key(&tpl.get_tpl_id())
                            && !remaining.is_empty()
                            && remaining.iter().any(|t| !matches!(t, Unknown | Any))
                            && !remaining.iter().any(|t| {
                                matches!(
                                    t,
                                    Variadic(v)
                                        if matches!(
                                            v.as_ref(),
                                            VariadicType::Base(TplRef(inner))
                                                if inner.get_tpl_id() == tpl.get_tpl_id()
                                        )
                                )
                            })
                        {
                            if remaining.len() == 1 {
                                if !unify_call_bindings(
                                    model,
                                    &TplRef(tpl.clone()),
                                    &remaining[0],
                                    bindings,
                                ) {
                                    return false;
                                }
                            } else {
                                let tuple_ty = Tuple(Arc::new(LuaTupleType::new(
                                    remaining,
                                    LuaTupleStatus::InferResolve,
                                )));
                                if !unify_call_bindings(
                                    model,
                                    &TplRef(tpl.clone()),
                                    &tuple_ty,
                                    bindings,
                                ) {
                                    return false;
                                }
                            }
                        }
                        // The variadic slot only consumes actual function params; the param index
                        // still advances by 1 (later ordinary params like `cb` may still match).
                        i += 1;
                        arg_index += consume.max(1);
                        continue;
                    }
                    let arg_ty = arg_params
                        .get(arg_index)
                        .and_then(|p| p.1.clone())
                        .unwrap_or(Unknown);
                    if !unify_call_bindings(model, &param_ty, &arg_ty, bindings) {
                        return false;
                    }
                    i += 1;
                    arg_index += 1;
                }
                // When the actual function is the same generic function value (e.g.
                // `wrap(wrap, ...)`), the param return R... and actual return R... share the
                // same TplRefs. Structurally unifying directly would bind R to a recursive type
                // containing R itself; leave that to the later apply callback call to infer.
                let actual_generic_ids: HashSet<GenericTplId> = arg_fun
                    .get_generic_params()
                    .iter()
                    .map(|p| p.get_tpl_id())
                    .collect();
                if !actual_generic_ids.is_empty()
                    && (type_contains_any_tpl_id(param_fun.get_ret(), &actual_generic_ids)
                        || param_fun.get_params().iter().any(|(_, ty)| {
                            ty.as_ref()
                                .is_some_and(|t| type_contains_any_tpl_id(t, &actual_generic_ids))
                        }))
                {
                    true
                } else {
                    let arg_ret = resolve_function_ret_defaults(arg_fun.get_ret(), arg_fun);
                    unify_call_bindings(model, param_fun.get_ret(), &arg_ret, bindings)
                }
            } else if matches!(arg, Function) {
                // Erased `function` can be a fallback for any callback param.
                true
            } else {
                false
            }
        }
        Union(param_union) => {
            if let Union(arg_union) = arg {
                let mut pv = param_union.into_vec();
                let mut av = arg_union.into_vec();
                if pv.contains(&Nil) && av.contains(&Nil) {
                    pv.retain(|t| !matches!(t, Nil));
                    av.retain(|t| !matches!(t, Nil));
                }
                if pv.len() == av.len() {
                    for (p, a) in pv.iter().zip(av.iter()) {
                        if !unify_call_bindings(model, p, a, bindings) {
                            return false;
                        }
                    }
                    true
                } else if pv.len() == 1 {
                    av.iter()
                        .any(|a| unify_call_bindings(model, &pv[0], a, bindings))
                } else {
                    false
                }
            } else {
                let pv = param_union.into_vec();
                // A generic union like `` `T`|T `` allows T to be bound through any generic
                // component; ordinary unions still require all non-nil components to be
                // compatible (caller may pass any member).
                if matches!(arg, Function | DocFunction(_) | Signature(_)) {
                    pv.iter()
                        .any(|p| unify_call_bindings(model, p, arg, bindings))
                } else if let Some(str_tpl) = pv.iter().find(|p| matches!(p, StrTplRef(_))) {
                    // `` `T`|T ``: string literals prefer the template component, resolving "A"
                    // to a named type instead of widening through the ordinary TplRef to string.
                    if !unify_call_bindings(model, str_tpl, arg, bindings) {
                        pv.iter()
                            .any(|p| unify_call_bindings(model, p, arg, bindings))
                    } else {
                        true
                    }
                } else if pv.iter().any(|p| matches!(p, TplRef(_))) {
                    pv.iter()
                        .any(|p| unify_call_bindings(model, p, arg, bindings))
                } else if pv.contains(&Nil) {
                    pv.iter()
                        .filter(|t| !matches!(t, Nil))
                        .all(|p| unify_call_bindings(model, p, arg, bindings))
                } else {
                    false
                }
            }
        }
        SelfInfer => {
            matches!(
                arg,
                SelfInfer
                    | Ref(_)
                    | Def(_)
                    | Generic(_)
                    | Table
                    | TableConst(_)
                    | TableGeneric(_)
                    | Object(_)
                    | Intersection(_)
            ) || matches!(arg, Unknown | Any)
        }
        StrTplRef(str_tpl) => {
            let value = match arg {
                StringConst(s) | DocStringConst(s) => Some(format!(
                    "{}{}{}",
                    str_tpl.get_prefix(),
                    s.as_str(),
                    str_tpl.get_suffix()
                )),
                _ => None,
            };
            let Some(full_name) = value else {
                return false;
            };
            let type_id = if let Some(def) = model.resolve_type_def(&full_name) {
                if matches!(def.visibility, TypeVisibility::Public) {
                    LuaTypeDeclId::global(&def.full_name)
                } else {
                    LuaTypeDeclId::file(def.file_id, &def.full_name)
                }
            } else {
                LuaTypeDeclId::global(&full_name)
            };
            bindings.insert(str_tpl.get_tpl_id(), Ref(type_id));
            true
        }
        _ => {
            if matches!(arg, Unknown | Any)
                || (param == arg || primitive_accepts_const(param, arg))
                || (matches!(param, Function) && matches!(arg, DocFunction(_) | Signature(_)))
            {
                true
            } else {
                unify::unify_bindings(param, arg, bindings)
            }
        }
    }
}

pub(crate) fn score_param_match(param_ty: &LuaType, arg_ty: &LuaType) -> i32 {
    use LuaType::*;
    match (param_ty, arg_ty) {
        (DocFunction(_), DocFunction(_)) => 100,
        (TplRef(_), _) => 50,
        (Any, _) | (Unknown, _) => 0,
        (IntegerConst(a), IntegerConst(b)) if a == b => 200,
        (IntegerConst(a), DocIntegerConst(b)) if a == b => 200,
        (DocIntegerConst(a), IntegerConst(b)) if a == b => 200,
        (DocIntegerConst(a), DocIntegerConst(b)) if a == b => 200,
        (StringConst(a), StringConst(b)) if a == b => 200,
        (StringConst(a), DocStringConst(b)) if a == b => 200,
        (DocStringConst(a), StringConst(b)) if a == b => 200,
        (DocStringConst(a), DocStringConst(b)) if a == b => 200,
        (Variadic(_), _) => 2,
        (a, b) if a == b => 80,
        _ => 20,
    }
}

pub(crate) fn variadic_base_type(ty: &LuaType) -> Option<LuaType> {
    match ty {
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Base(base) => Some(base.clone()),
            VariadicType::Multi(types) => types.first().cloned(),
        },
        _ => Some(ty.clone()),
    }
}

/// Whether the call return's bare `T` comes from a `---@param ... T...` variadic-sequence
/// generic. If so, when T is bound to a tuple, the function return should expand to that
/// tuple's multi-return slots.
fn is_variadic_tuple_return(fun: &LuaFunctionType, _ret: &LuaType) -> bool {
    let LuaType::TplRef(ret_tpl) = fun.get_ret() else {
        return false;
    };
    let ret_tpl_id = ret_tpl.get_tpl_id();
    fun.get_params().iter().any(|(name, ty)| {
        (name == "..." || matches!(ty, Some(LuaType::Variadic(_))))
            && matches!(
                ty,
                Some(LuaType::Variadic(variadic))
                    if matches!(
                        variadic.as_ref(),
                        VariadicType::Base(LuaType::TplRef(param_tpl))
                            if param_tpl.get_tpl_id() == ret_tpl_id
                    )
            )
    })
}

// ──────────────────────────────────────────────
// Convenience: compile + run
// ──────────────────────────────────────────────

/// Build a `LuaFunctionType` from an inline `fun<T>(...): ...` doc type (rich projection
/// version).
pub fn infer_doc_func(
    model: &SemanticModel,
    file_id: FileId,
    type_syntax: LuaSyntaxId,
) -> Option<LuaFunctionType> {
    let vm = InferVm::new(model, &[]);
    vm.doc_func_from_syntax(file_id, type_syntax, &[], false)
}

/// Infer an expression (VM): compile -> interpret. Result = LuaType.
pub fn infer_expr_vm(model: &SemanticModel, expr_syntax: LuaSyntaxId) -> LuaType {
    let Some(tree) = model.syntax_tree() else {
        return LuaType::Unknown;
    };
    let root = tree.get_red_root();
    let Some(node) = expr_syntax.to_node_from_root(&root) else {
        return LuaType::Unknown;
    };
    let Some(expr) = LuaExpr::cast(node) else {
        return LuaType::Unknown;
    };
    let mut code = Vec::new();
    compile(expr, model.file_id(), &mut code);
    code.push(Instr::Result);
    InferVm::new(model, &code).run()
}

/// Extract the callback function type from a param type, supporting `fun(...)` and unions
/// containing function components (`string | fun(...)`).
fn callback_fun_from_param(model: &SemanticModel, ty: &LuaType) -> Option<LuaFunctionType> {
    let ty = crate::semantic_model::type_eval::expand_alias_generic(model, ty);
    match ty {
        LuaType::DocFunction(fun) => Some(fun.as_ref().clone()),
        LuaType::Ref(id) | LuaType::Def(id)
            if model
                .type_def_of(&id)
                .and_then(|def| (def.kind == TypeDefKind::Alias).then_some(def))
                .and_then(|def| model.alias_target(&def))
                .is_some() =>
        {
            let def = model.type_def_of(&id)?;
            callback_fun_from_param(model, &model.alias_target(&def)?)
        }
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .find_map(|component| callback_fun_from_param(model, component)),
        _ => None,
    }
}

/// Closure param type: compile the "call that wraps this closure" and run it, then read the
/// param environment.
pub fn closure_param_vm(
    model: &SemanticModel,
    closure_syntax: LuaSyntaxId,
    param_index: usize,
) -> LuaType {
    let Some(tree) = model.syntax_tree() else {
        return LuaType::Unknown;
    };
    let root = tree.get_red_root();
    let Some(node) = closure_syntax.to_node_from_root(&root) else {
        return LuaType::Unknown;
    };
    let Some(closure) = LuaClosureExpr::cast(node) else {
        return LuaType::Unknown;
    };
    if model.is_in_closure_return_infer(closure_syntax) {
        return LuaType::Unknown;
    }
    // Find the wrapping call.
    let Some(call_expr) = closure.ancestors::<LuaCallExpr>().next() else {
        return LuaType::Unknown;
    };
    let mut code = Vec::new();
    compile(LuaExpr::CallExpr(call_expr), model.file_id(), &mut code);
    code.push(Instr::Result);
    let mut vm = InferVm::new(model, &code);
    let _ = vm.run();
    vm.closure_params
        .get(&(closure_syntax, param_index))
        .cloned()
        .unwrap_or(LuaType::Unknown)
}

// ──────────────────────────────────────────────
// Utilities
// ──────────────────────────────────────────────

fn primitive_lua_type(p: PrimitiveType) -> LuaType {
    match p {
        PrimitiveType::Nil => LuaType::Nil,
        PrimitiveType::Boolean => LuaType::Boolean,
        PrimitiveType::Integer => LuaType::Integer,
        PrimitiveType::Number => LuaType::Number,
        PrimitiveType::String => LuaType::String,
        PrimitiveType::Table => LuaType::Table,
        PrimitiveType::Function => LuaType::Function,
        PrimitiveType::EmptyObject => LuaType::Object(Arc::new(LuaObjectType::new_with_fields(
            Default::default(),
            Vec::new(),
        ))),
    }
}

/// Whether `ty` is a `TableConst` from an empty table literal.
fn is_empty_table_literal(model: &SemanticModel, ty: &LuaType) -> bool {
    let LuaType::TableConst(table) = ty else {
        return false;
    };
    model
        .members_of_owner(&SemanticId::member(table.file_id, table.value))
        .is_empty()
}

/// Whether the class or its inheritance chain has a non-nullable named field (an empty
/// table literal cannot be a valid value of that class).
fn class_has_required_named_field(model: &SemanticModel, def: &TypeDef) -> bool {
    fn collect(model: &SemanticModel, def: &TypeDef, visited: &mut Vec<SemanticId>) -> bool {
        if visited.contains(&def.id) {
            return false;
        }
        visited.push(def.id.clone());
        for member_ref in model.members_of_owner(&def.id) {
            let Some(facts) = model.file_facts_of(member_ref.file_id) else {
                continue;
            };
            let Some(member) = facts.member_by_id(&member_ref.id) else {
                continue;
            };
            if !member.is_index_signature && !member.is_nullable {
                return true;
            }
        }
        for super_name in &def.super_names {
            let Some(super_def) = model
                .resolve_type_def_in(def.file_id, super_name.as_str())
                .or_else(|| {
                    model
                        .type_defs_in_scope(TypeScope::Global, super_name.as_str())
                        .into_iter()
                        .next()
                })
            else {
                continue;
            };
            if collect(model, &super_def, visited) {
                return true;
            }
        }
        false
    }
    collect(model, def, &mut Vec::new())
}

/// Whether an empty table literal is already covered by `target`'s truthy type. Unlike
/// ordinary `type_check`, this decides by "can fields be omitted": only table types where all
/// named fields are optional accept `{}`.
fn empty_table_fully_covered_by(model: &SemanticModel, target: &LuaType) -> bool {
    match target {
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .all(|component| empty_table_fully_covered_by(model, component)),
        LuaType::Ref(id) | LuaType::Def(id) => {
            let Some(def) = model.type_def_of(id) else {
                return false;
            };
            match def.kind {
                TypeDefKind::Alias => model
                    .alias_target(&def)
                    .map(|target| empty_table_fully_covered_by(model, &target))
                    .unwrap_or(false),
                TypeDefKind::Class => !class_has_required_named_field(model, &def),
                TypeDefKind::Enum => false,
            }
        }
        LuaType::Object(object) => object
            .get_fields()
            .values()
            .all(|field_ty| field_ty.is_nullable()),
        LuaType::Table
        | LuaType::TableGeneric(_)
        | LuaType::Array(_)
        | LuaType::Tuple(_)
        | LuaType::Generic(_)
        | LuaType::Unknown
        | LuaType::Any => true,
        _ => false,
    }
}

/// Whether every component of `ty` is assignable to `target` (the source-union
/// "any member assignable" semantics does not apply).
fn type_fully_covered_by(model: &SemanticModel, ty: &LuaType, target: &LuaType) -> bool {
    // An empty table literal cannot be absorbed by any class/class union merely through the
    // permissive "table assignable to class" rule: when required fields are missing, `x or {}`
    // should add plain `table` to the result rather than incorrectly narrow to the class.
    if is_empty_table_literal(model, ty) {
        return empty_table_fully_covered_by(model, target);
    }
    // When the target is a literal constant, only the exact same constant covers it, so
    // `Boolean` is not treated as a subtype of `true`.
    if matches!(
        target,
        LuaType::BooleanConst(_)
            | LuaType::DocBooleanConst(_)
            | LuaType::StringConst(_)
            | LuaType::DocStringConst(_)
            | LuaType::IntegerConst(_)
            | LuaType::DocIntegerConst(_)
    ) {
        return match ty {
            LuaType::Union(union) => union.into_vec().iter().all(|c| c == target),
            other => other == target,
        };
    }

    match ty {
        LuaType::Union(union) => union
            .into_vec()
            .iter()
            .all(|component| model.type_check(component, target)),
        other => model.type_check(other, target),
    }
}

/// Split `a and b` / `a or b` into truthy/falsy components.
fn truthy_components(ty: &LuaType) -> Vec<LuaType> {
    match ty {
        LuaType::Union(union) => union
            .into_vec()
            .into_iter()
            .flat_map(|t| truthy_components(&t))
            .collect(),
        LuaType::Boolean => vec![LuaType::BooleanConst(true)],
        LuaType::BooleanConst(b) => {
            if *b {
                vec![ty.clone()]
            } else {
                Vec::new()
            }
        }
        LuaType::DocBooleanConst(b) => {
            if *b {
                vec![ty.clone()]
            } else {
                Vec::new()
            }
        }
        LuaType::Nil => Vec::new(),
        LuaType::Unknown | LuaType::Any => vec![ty.clone()],
        other => vec![other.clone()],
    }
}

fn falsy_components(ty: &LuaType) -> Vec<LuaType> {
    match ty {
        LuaType::Union(union) => union
            .into_vec()
            .into_iter()
            .flat_map(|t| falsy_components(&t))
            .collect(),
        LuaType::Boolean => vec![LuaType::BooleanConst(false)],
        LuaType::BooleanConst(b) => {
            if *b {
                Vec::new()
            } else {
                vec![ty.clone()]
            }
        }
        LuaType::DocBooleanConst(b) => {
            if *b {
                Vec::new()
            } else {
                vec![ty.clone()]
            }
        }
        LuaType::Nil => vec![ty.clone()],
        LuaType::Unknown | LuaType::Any => vec![ty.clone()],
        _ => Vec::new(),
    }
}

pub(crate) fn binary_type(
    model: &SemanticModel,
    op: BinaryOperator,
    left: &LuaType,
    right: &LuaType,
) -> LuaType {
    use BinaryOperator::*;
    // Operator-overload dispatch: check the left operand first, then the right operand.
    if let Some(name) = binary_operator_name(op) {
        if let Some(ty) = model.operator_type(name, left) {
            return ty;
        }
        if let Some(ty) = model.operator_type(name, right) {
            return ty;
        }
    }
    // The old path treats RHS array tables in `x and { 'a' }` as tuples.
    let right = if matches!(op, OpAnd | OpOr) {
        table_literal_as_tuple(model, right).unwrap_or_else(|| right.clone())
    } else {
        right.clone()
    };
    match op {
        // Short-circuit evaluation: `a and b` = falsy(a) | b; `a or b` = truthy(a) | b.
        OpAnd => {
            if matches!(left, LuaType::Unknown) {
                LuaType::Nil
            } else if left.is_always_falsy() {
                left.clone()
            } else if left.is_always_truthy() {
                right.clone()
            } else {
                let mut types = falsy_components(left);
                types.push(right.clone());
                LuaType::from_vec(types)
            }
        }
        OpOr => {
            if left.is_always_truthy() {
                left.clone()
            } else if left.is_always_falsy() {
                right.clone()
            } else {
                let left_truthy = LuaType::from_vec(truthy_components(left));
                // `x or y`: if y is already covered by x (or its truthy part), narrow directly
                // to x's truthy part. Enum/class types `T? or T.A` also narrow to T directly,
                // avoiding false positives from unioning enum member literals.
                let right_is_enum_literal =
                    matches!(
                        &right,
                        LuaType::Integer
                            | LuaType::Number
                            | LuaType::String
                            | LuaType::IntegerConst(_)
                            | LuaType::DocIntegerConst(_)
                            | LuaType::StringConst(_)
                            | LuaType::DocStringConst(_)
                    ) && matches!(left_truthy, LuaType::Ref(_) | LuaType::Def(_));
                if type_fully_covered_by(model, &right, &left_truthy) || right_is_enum_literal {
                    left_truthy
                } else {
                    let mut types = truthy_components(left);
                    types.push(right.clone());
                    LuaType::from_vec(types)
                }
            }
        }
        OpConcat => LuaType::String,
        OpLt | OpLe | OpGt | OpGe | OpEq | OpNe => {
            // Integer-const comparison folding: `1 < 2` stays BooleanConst (consistent with
            // the old path).
            if let (Some(left_i), Some(right_i)) =
                (integer_const_value(left), integer_const_value(&right))
            {
                let result = match op {
                    OpLt => left_i < right_i,
                    OpLe => left_i <= right_i,
                    OpGt => left_i > right_i,
                    OpGe => left_i >= right_i,
                    OpEq => left_i == right_i,
                    OpNe => left_i != right_i,
                    _ => false,
                };
                return LuaType::BooleanConst(result);
            }
            LuaType::Boolean
        }
        OpAdd | OpSub | OpMul => {
            // When `any` participates in arithmetic, the result stays `any` (TS any
            // propagation semantics); do not silently degrade `a.anyField / 100 * b.anyField`
            // to Number.
            if left.is_any() || right.is_any() {
                return LuaType::Any;
            }
            // Integer-const folding: `1 + 1` stays IntegerConst (consistent with deep-chain
            // tests and the old path).
            if let (Some(left_i), Some(right_i)) =
                (integer_const_value(left), integer_const_value(&right))
            {
                let value = match op {
                    OpAdd => left_i.checked_add(right_i),
                    OpSub => left_i.checked_sub(right_i),
                    OpMul => left_i.checked_mul(right_i),
                    _ => None,
                };
                return value.map(LuaType::IntegerConst).unwrap_or(LuaType::Number);
            }
            // Addition like `integer + 1` (one side is the integer base type, the other is an
            // integer literal) still stays integer.
            let left_is_integer = matches!(
                left,
                LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
            );
            let right_is_integer = matches!(
                right,
                LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_)
            );
            if left_is_integer && right_is_integer {
                LuaType::Integer
            } else {
                LuaType::Number
            }
        }
        OpDiv | OpIDiv | OpMod | OpPow => {
            if left.is_any() || right.is_any() {
                LuaType::Any
            } else {
                LuaType::Number
            }
        }
        _ => LuaType::Unknown,
    }
}

/// In short-circuit logical expressions, if the left/right side is a nullable field
/// (`@field x? T`), add nil back. This logic only applies on the short-circuit evaluation
/// path, avoiding breaking existing narrowing tests after globally changing member base types.
fn logical_left_nullable_extra(model: &SemanticModel, syntax: LuaSyntaxId) -> Option<LuaType> {
    let tree = model.syntax_tree()?;
    let node = syntax.to_node_from_root(&tree.get_red_root())?;
    let LuaExpr::IndexExpr(index_expr) = LuaExpr::cast(node)? else {
        return None;
    };
    let resolved = model.resolve_member(&index_expr)?;
    let member_id = resolved.member_id?;
    let member_file = match &member_id {
        SemanticId::Member(key) => key.file_id,
        _ => model.file_id(),
    };
    let facts = model.file_facts_of(member_file)?;
    let member = facts.member_by_id(&member_id)?;
    member.is_nullable.then_some(LuaType::Nil)
}

/// Simple dense array table literal -> tuple (old path's `{ 'a' }` semantics in
/// short-circuit expressions).
fn table_literal_as_tuple(model: &SemanticModel, ty: &LuaType) -> Option<LuaType> {
    let LuaType::TableConst(table) = ty else {
        return None;
    };
    let owner = SemanticId::member(table.file_id, table.value);
    let member_refs = model.members_of_owner(&owner);
    if member_refs.is_empty() {
        return None;
    }
    let mut entries = Vec::with_capacity(member_refs.len());
    for member_ref in member_refs {
        let facts = model.file_facts_of(member_ref.file_id)?;
        let member = facts.member_by_id(&member_ref.id)?;
        let LuaMemberKey::Integer(key) = member.key else {
            return None;
        };
        if key <= 0 {
            return None;
        }
        let value_syntax = member.value_syntax?;
        entries.push((key, model.type_of_expr(value_syntax)));
    }
    entries.sort_by_key(|(key, _)| *key);
    if entries
        .iter()
        .enumerate()
        .any(|(index, (key, _))| *key != index as i64 + 1)
    {
        return None;
    }
    Some(LuaType::Tuple(Arc::new(LuaTupleType::new(
        entries.into_iter().map(|(_, ty)| ty).collect(),
        LuaTupleStatus::InferResolve,
    ))))
}

/// Reassign a function type's own generics from `GenericTplId::Type` to
/// `GenericTplId::Func`, avoiding ID conflicts with outer function generics (still `Type`) in
/// the same `TplBindings`, thereby implementing `fun<T>`'s internal T shadowing an outer
/// `---@generic T`.
pub(crate) fn reassign_function_generics_to_func_ids(fun: LuaFunctionType) -> LuaFunctionType {
    let old_generics = fun.get_generic_params().to_vec();
    if old_generics.is_empty() {
        return fun;
    }
    let func_generics: Vec<GenericTpl> = old_generics
        .iter()
        .enumerate()
        .map(|(index, old)| {
            let param = old.get_param();
            GenericTpl::new(
                GenericTplId::Func(index as u32),
                param.name.clone(),
                param.constraint.clone(),
                param.default.clone(),
                param.is_const,
                param.attributes.clone(),
            )
        })
        .collect();
    let params = fun
        .get_params()
        .iter()
        .map(|(name, ty)| {
            (
                name.clone(),
                ty.as_ref()
                    .map(|t| bind_signature_generics(t, &func_generics)),
            )
        })
        .collect();
    let ret = bind_signature_generics(fun.get_ret(), &func_generics);
    LuaFunctionType::new(
        fun.get_async_state(),
        fun.is_colon_define(),
        fun.is_variadic(),
        params,
        ret,
        Some(func_generics),
    )
}

/// Restore generic refs projected as `Ref("T")` in signature docs back to TplRef
/// (recursively through Object structures).
pub(crate) fn bind_signature_generics(ty: &LuaType, generics: &[GenericTpl]) -> LuaType {
    match ty {
        LuaType::TplRef(tpl) => {
            if let Some(generic) = generics.iter().find(|generic| {
                generic.get_tpl_id() == tpl.get_tpl_id() || generic.get_name() == tpl.get_name()
            }) {
                LuaType::TplRef(Arc::new(generic.clone()))
            } else {
                ty.clone()
            }
        }
        LuaType::Ref(id) | LuaType::Def(id) => {
            let name = id.get_name();
            if let Some((index, generic)) = generics
                .iter()
                .enumerate()
                .find(|(_, generic)| generic.get_name() == name)
            {
                return LuaType::TplRef(Arc::new(GenericTpl::new(
                    GenericTplId::Type(index as u32),
                    SmolStr::new(name),
                    generic.get_constraint().cloned(),
                    generic.get_default_type().cloned(),
                    generic.is_const(),
                    None,
                )));
            }
            ty.clone()
        }
        LuaType::Call(call) => LuaType::Call(Arc::new(crate::LuaAliasCallType::new(
            call.get_call_kind(),
            call.get_operands()
                .iter()
                .map(|t| bind_signature_generics(t, generics))
                .collect(),
        ))),
        LuaType::Mapped(mapped) => LuaType::Mapped(Arc::new(crate::LuaMappedType::new(
            (
                mapped.param.0,
                crate::GenericParam::new(
                    mapped.param.1.name.clone(),
                    mapped
                        .param
                        .1
                        .constraint
                        .as_ref()
                        .map(|t| bind_signature_generics(t, generics)),
                    mapped
                        .param
                        .1
                        .default
                        .as_ref()
                        .map(|t| bind_signature_generics(t, generics)),
                    mapped.param.1.is_const,
                    mapped.param.1.attributes.clone(),
                ),
            ),
            bind_signature_generics(&mapped.value, generics),
            mapped.is_readonly,
            mapped.is_optional,
        ))),
        LuaType::Generic(generic) => LuaType::Generic(Arc::new(LuaGenericType::new(
            generic.get_base_type_id(),
            generic
                .get_params()
                .iter()
                .map(|t| bind_signature_generics(t, generics))
                .collect(),
        ))),
        LuaType::Conditional(conditional) => {
            LuaType::Conditional(Arc::new(crate::LuaConditionalType::new(
                bind_signature_generics(conditional.get_checked_type(), generics),
                bind_signature_generics(conditional.get_extends_type(), generics),
                bind_signature_generics(conditional.get_true_type(), generics),
                bind_signature_generics(conditional.get_false_type(), generics),
                conditional.get_infer_params().to_vec(),
                conditional.has_new,
            )))
        }
        LuaType::TableGeneric(generic) => LuaType::TableGeneric(Arc::new(
            generic
                .iter()
                .map(|t| bind_signature_generics(t, generics))
                .collect(),
        )),
        LuaType::Intersection(intersection) => {
            LuaType::Intersection(Arc::new(crate::LuaIntersectionType::new(
                intersection
                    .get_types()
                    .iter()
                    .map(|t| bind_signature_generics(t, generics))
                    .collect(),
            )))
        }
        LuaType::Object(object) => {
            let fields = object
                .get_fields()
                .iter()
                .map(|(key, value)| (key.clone(), bind_signature_generics(value, generics)))
                .collect();
            LuaType::Object(Arc::new(LuaObjectType::new_with_fields(
                fields,
                object.get_index_access().to_vec(),
            )))
        }
        LuaType::Union(union) => LuaType::Union(Arc::new(LuaUnionType::from_vec(
            union
                .into_vec()
                .iter()
                .map(|component| bind_signature_generics(component, generics))
                .collect(),
        ))),
        LuaType::Array(array) => LuaType::Array(Arc::new(LuaArrayType::from_base_type(
            bind_signature_generics(array.get_base(), generics),
        ))),
        LuaType::Variadic(variadic) => {
            let resolved = match variadic.as_ref() {
                VariadicType::Base(base) => {
                    VariadicType::Base(bind_signature_generics(base, generics))
                }
                VariadicType::Multi(types) => VariadicType::Multi(
                    types
                        .iter()
                        .map(|component| bind_signature_generics(component, generics))
                        .collect(),
                ),
            };
            LuaType::Variadic(Arc::new(resolved))
        }
        LuaType::DocFunction(fun) => {
            // Function types with their own generic declarations (internal IDs are Func(...))
            // have their own generic scope; outer generics must not be substituted inside them.
            // Only function types inheriting the outer context (Type IDs) continue substituting.
            if fun
                .get_generic_params()
                .iter()
                .any(|param| param.get_tpl_id().is_func())
            {
                return LuaType::DocFunction(fun.clone());
            }
            let params = fun
                .get_params()
                .iter()
                .map(|(name, ty)| {
                    (
                        name.clone(),
                        ty.as_ref().map(|t| bind_signature_generics(t, generics)),
                    )
                })
                .collect();
            let ret = bind_signature_generics(fun.get_ret(), generics);
            LuaType::DocFunction(Arc::new(LuaFunctionType::new(
                fun.get_async_state(),
                fun.is_colon_define(),
                fun.is_variadic(),
                params,
                ret,
                Some(fun.get_generic_params().to_vec()),
            )))
        }
        _ => ty.clone(),
    }
}

fn integer_const_value(ty: &LuaType) -> Option<i64> {
    match ty {
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => Some(*i),
        _ => None,
    }
}

fn is_enum_like_dynamic_key(model: &SemanticModel, ty: &LuaType) -> bool {
    let (LuaType::Ref(id) | LuaType::Def(id)) = ty else {
        return false;
    };
    let Some(def) = model.type_def_of(id) else {
        return false;
    };
    matches!(def.kind, TypeDefKind::Enum)
}

/// `---@enum T` usually decorates a runtime table (`local Op = {...}`); use
/// `def.owner_syntax` to find the corresponding declaration, on which enum members hang.
fn enum_runtime_owner(model: &SemanticModel, def: &TypeDef) -> Option<SemanticId> {
    let facts = model.file_facts_of(def.file_id)?;
    facts.decls.iter().find_map(|decl| {
        if decl.owner_syntax == def.owner_syntax {
            // A global enum table `Op = { ... }`'s fields hang on the synthetic table identity.
            let value_syntax = decl.value_expr_syntax?;
            Some(SemanticId::member(def.file_id, value_syntax.get_range()))
        } else {
            None
        }
    })
}

/// Unary operation (including operator overloads unm/len).
fn unary_type(
    model: &SemanticModel,
    op: emmylua_parser::UnaryOperator,
    operand: &LuaType,
) -> LuaType {
    use emmylua_parser::UnaryOperator::*;
    let name = match op {
        OpUnm => Some("unm"),
        OpLen => Some("len"),
        _ => None,
    };
    if let Some(name) = name
        && let Some(ty) = model.operator_type(name, operand)
    {
        return ty;
    }
    operand.clone()
}

/// Binary operator -> `@operator` name.
fn binary_operator_name(op: BinaryOperator) -> Option<&'static str> {
    use BinaryOperator::*;
    Some(match op {
        OpAdd => "add",
        OpSub => "sub",
        OpMul => "mul",
        OpDiv => "div",
        OpMod => "mod",
        OpPow => "pow",
        OpConcat => "concat",
        OpEq => "eq",
        OpLt => "lt",
        OpLe => "le",
        _ => return None,
    })
}

/// Remove nil from a type (for `StringConst` / `DocStringConst` extractions).
fn remove_nil_from_type(ty: LuaType) -> LuaType {
    match ty {
        LuaType::Union(union) => {
            let types = union
                .into_vec()
                .into_iter()
                .filter(|t| !matches!(t, LuaType::Nil))
                .collect::<Vec<_>>();
            LuaType::from_vec(types)
        }
        LuaType::Nil => LuaType::Unknown,
        LuaType::Variadic(variadic) => match variadic.as_ref() {
            VariadicType::Multi(types) => {
                let mut new_types = types.clone();
                if let Some(first) = new_types.first_mut() {
                    *first = remove_nil_from_type(first.clone());
                }
                LuaType::Variadic(Arc::new(VariadicType::Multi(new_types)))
            }
            VariadicType::Base(base) => LuaType::Variadic(Arc::new(VariadicType::Base(
                remove_nil_from_type(base.clone()),
            ))),
        },
        _ => ty,
    }
}

fn string_const_of(ty: &LuaType) -> Option<String> {
    match ty {
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => Some(s.as_ref().to_string()),
        _ => None,
    }
}

fn merge(a: LuaType, b: LuaType) -> LuaType {
    if a == b {
        return a;
    }
    let mut types = Vec::new();
    match &a {
        LuaType::Union(union) => types.extend(union.into_vec()),
        _ => types.push(a),
    }
    match &b {
        LuaType::Union(union) => types.extend(union.into_vec()),
        _ => types.push(b),
    }
    LuaType::Union(Arc::new(LuaUnionType::from_vec(types)))
}
