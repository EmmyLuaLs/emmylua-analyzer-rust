//! # CompletionBuilder — Completion builder
//!
//! Unified carrier for the salsa model / document view / config, providing the
//! context detection and common completion item writes required by the provider
//! pipeline. Fields remain consistent with the old provider pipeline
//! (`trigger_token` / `context` / `env_duplicate_name` / `is_space_trigger_character`),
//! with the semantic model replaced by `SalsaSemanticModel`.

use std::collections::HashSet;
use std::sync::Arc;

use emmylua_code_analysis::{DocumentView, Emmyrc, LuaType, SalsaSemanticModel};
use emmylua_parser::{LuaAstNode, LuaSyntaxToken, LuaTokenKind};
use lsp_types::{CompletionItem, CompletionTriggerKind};
use rowan::TextSize;
use tokio_util::sync::CancellationToken;

use super::providers::CompletionContext;

pub struct CompletionBuilder<'a> {
    pub trigger_token: LuaSyntaxToken,
    pub semantic_model: SalsaSemanticModel<'a>,
    pub document: Arc<DocumentView>,
    pub emmyrc: Arc<Emmyrc>,
    pub context: CompletionContext,
    pub env_duplicate_name: HashSet<String>,
    pub trigger_kind: CompletionTriggerKind,
    /// Whether completion is triggered by a space/whitespace character (not explicitly invoked).
    pub is_space_trigger_character: bool,
    pub position_offset: TextSize,
    completion_items: Vec<CompletionItem>,
    cancel_token: CancellationToken,
}

impl<'a> CompletionBuilder<'a> {
    pub fn new(
        trigger_token: LuaSyntaxToken,
        semantic_model: SalsaSemanticModel<'a>,
        document: Arc<DocumentView>,
        emmyrc: Arc<Emmyrc>,
        trigger_kind: CompletionTriggerKind,
        position_offset: TextSize,
        cancel_token: CancellationToken,
    ) -> Self {
        let is_space_trigger_character = trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER
            && trigger_token.text().trim_end().is_empty();

        let mut builder = Self {
            trigger_token,
            semantic_model,
            document,
            emmyrc,
            context: CompletionContext::General,
            env_duplicate_name: HashSet::new(),
            trigger_kind,
            is_space_trigger_character,
            position_offset,
            completion_items: Vec::new(),
            cancel_token,
        };
        builder.context = CompletionContext::analyze(&builder);
        builder
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    pub fn add_completion_item(&mut self, item: CompletionItem) -> Option<()> {
        self.completion_items.push(item);
        Some(())
    }

    pub fn get_completion_items(self) -> Vec<CompletionItem> {
        self.completion_items
    }

    pub fn get_completion_items_mut(&mut self) -> &mut Vec<CompletionItem> {
        &mut self.completion_items
    }

    pub fn get_trigger_text(&self) -> String {
        self.trigger_token.text().trim_end().to_string()
    }

    /// Explicitly invoked completion.
    pub fn is_invoked(&self) -> bool {
        self.trigger_kind == CompletionTriggerKind::INVOKED
    }

    /// Current partial name text (name completion prefix).
    pub fn partial_name(&self) -> String {
        if matches!(self.trigger_token.kind().into(), LuaTokenKind::TkName) {
            self.get_trigger_text()
        } else {
            String::new()
        }
    }

    pub fn get_emmyrc(&self) -> &Emmyrc {
        &self.emmyrc
    }

    pub fn get_emmyrc_arc(&self) -> Arc<Emmyrc> {
        self.emmyrc.clone()
    }

    pub fn get_document(&self) -> &DocumentView {
        &self.document
    }

    /// Use a function call snippet when the type is a function and call_snippet is enabled.
    pub fn support_snippets(&self, ty: &LuaType) -> bool {
        ty.is_function() && self.emmyrc.completion.call_snippet
    }

    /// Whether this is a member completion context (`.` or `:` before the offset).
    #[allow(unused)]
    pub fn is_member_completion(&self) -> bool {
        matches!(
            self.trigger_token.kind().into(),
            LuaTokenKind::TkDot | LuaTokenKind::TkColon
        ) || self.trigger_token.prev_token().is_some_and(|token| {
            matches!(
                token.kind().into(),
                LuaTokenKind::TkDot | LuaTokenKind::TkColon
            )
        })
    }

    /// Type of the prefix expression for member completion (expression before `.` or `:`).
    pub fn member_prefix_type(&self) -> Option<LuaType> {
        let chunk = self.semantic_model.chunk()?;
        let token = chunk
            .syntax()
            .token_at_offset(self.position_offset)
            .left_biased()?;
        let dot = if matches!(
            token.kind().into(),
            LuaTokenKind::TkDot | LuaTokenKind::TkColon
        ) {
            token
        } else {
            token.prev_token()?
        };
        let parent = dot.parent()?;
        let index_expr = emmylua_parser::LuaIndexExpr::cast(parent)?;
        let prefix = index_expr.get_prefix_expr()?;
        let ty = self.semantic_model.type_of_expr(prefix.get_syntax_id());

        // Method chain `B:one():<??>`: VM may give Unknown for `return self` without docs;
        // fall back to the receiver identity (B's runtime table/class) so member completion can still list methods.
        if matches!(ty, LuaType::Unknown)
            && let emmylua_parser::LuaExpr::CallExpr(call) = &prefix
        {
            // Return type fallback for class/generic function calls: `expect(...):<??>`.
            if let Some(ret) = call_return_type(&self.semantic_model, call) {
                return Some(ret);
            }
            let Some(callee_expr) = call.get_prefix_expr() else {
                return Some(ty);
            };
            let Some(callee_index) =
                emmylua_parser::LuaIndexExpr::cast(callee_expr.syntax().clone())
            else {
                return Some(ty);
            };
            if let Some(resolved) = self.semantic_model.resolve_member(&callee_index) {
                // Cross-file `B:one` may have no member declaration id; fall back to the owner type name.
                if resolved.member_id.is_none() {
                    if let emmylua_code_analysis::SemanticId::Name(owner_name) = &resolved.owner
                        && let Some(def) = self.semantic_model.resolve_type_def(owner_name)
                    {
                        let receiver = self.semantic_model.type_def_ref(&def);
                        if self
                            .semantic_model
                            .member_infos(&receiver)
                            .iter()
                            .any(|info| info.key.to_path() == resolved.name && info.is_method)
                        {
                            return Some(receiver);
                        }
                        // Runtime methods may be defined in declaration files (`function B:one()`);
                        // across files member_infos may be empty, so scan the definition file's facts directly.
                        if let Some(facts) = self.semantic_model.file_facts_of(def.file_id) {
                            let has_method = facts.members.iter().any(|member| {
                                member.key.to_path() == resolved.name && member.is_method
                            });
                            if has_method {
                                return Some(receiver);
                            }
                        }
                    }
                }
                let receiver_ty = callee_index
                    .get_prefix_expr()
                    .map(|expr| self.semantic_model.type_of_expr(expr.get_syntax_id()));
                let receiver_ty = match receiver_ty {
                    Some(ty) if !matches!(ty, LuaType::Unknown) => Some(ty),
                    _ => {
                        // Cross-file global class: when `---@class B B = {}` is in another file,
                        // the runtime value type may project to Unknown; fall back by type name.
                        let prefix_expr = callee_index.get_prefix_expr()?;
                        let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix_expr else {
                            return None;
                        };
                        let name = name_expr.get_name_text()?;
                        self.semantic_model
                            .resolve_type_def(&name)
                            .map(|def| self.semantic_model.type_def_ref(&def))
                    }
                };
                if resolved.is_method
                    && let Some(receiver_ty) = receiver_ty
                    && !matches!(receiver_ty, LuaType::Unknown)
                {
                    return Some(receiver_ty);
                }
            }
        }

        Some(ty)
    }

    /// Type of an assignment target member (`on_add` in `c1.on_add = <??>`), used for function implementation completion.
    pub fn assignment_target_member_type(&self) -> Option<LuaType> {
        let chunk = self.semantic_model.chunk()?;
        let probe = self
            .position_offset
            .checked_sub(1.into())
            .unwrap_or(self.position_offset);
        let assign_stat = chunk
            .syntax()
            .descendants()
            .filter_map(emmylua_parser::LuaAssignStat::cast)
            .find(|assign| assign.get_range().contains_inclusive(probe))?;
        let (vars, _) = assign_stat.get_var_and_expr_list();
        let var = vars.first()?;
        let emmylua_parser::LuaVarExpr::IndexExpr(index_expr) = var else {
            return None;
        };
        let key_text = index_expr.get_index_key()?.get_path_part();

        // 1. Normal member resolution.
        if let Some(resolved) = self.semantic_model.resolve_member(index_expr) {
            if let Some(member_id) = resolved.member_id
                && let Some(ty) = self.semantic_model.type_of_member(&member_id)
                && !matches!(ty, LuaType::Unknown)
            {
                return Some(ty);
            }
        }

        // 2. `---@class X` directly following `local a` (class name differs from variable / local lacks @type):
        // associate the type definition via the owner statement, then find the member with the same key from the type and runtime declarations.
        let prefix = index_expr.get_prefix_expr()?;
        let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix else {
            return None;
        };
        let decl = self.semantic_model.resolve_name(name_expr.get_position())?;
        let facts = self.semantic_model.file_facts()?;
        let decl_info = facts.decl_by_id(&decl)?;
        let def = facts
            .type_defs
            .iter()
            .find(|def| def.owner_syntax.is_some() && def.owner_syntax == decl_info.owner_syntax)?;

        // Members from the inheritance chain (`set` from `MHandler : ProxyHandler` is defined on the parent type).
        for info in self
            .semantic_model
            .member_infos(&self.semantic_model.type_def_ref(def))
        {
            if info.key.to_path() == key_text
                && let Some(id) = &info.id
                && let Some(ty) = self.semantic_model.type_of_member(id)
                && !matches!(ty, LuaType::Unknown)
            {
                return Some(ty);
            }
        }

        for owner in [def.id.clone(), decl] {
            for member_ref in self.semantic_model.members_of_owner(&owner) {
                let member_facts = self.semantic_model.file_facts_of(member_ref.file_id)?;
                let member = member_facts.member_by_id(&member_ref.id)?;
                if member.key.to_path() == key_text {
                    if let Some(ty) = self.semantic_model.type_of_member(&member_ref.id)
                        && !matches!(ty, LuaType::Unknown)
                    {
                        return Some(ty);
                    }
                    // `@field set? ProxyHandler.Setter`: when type projection fails, use the doc type node directly.
                    if let Some(doc_type) = member.doc_type_syntax {
                        let ty =
                            self.semantic_model
                                .doc_type_lua_in(member_ref.file_id, doc_type, &[]);
                        if !matches!(ty, LuaType::Unknown) {
                            return Some(ty);
                        }
                    }
                }
            }
        }
        None
    }
}

/// Call expression return type fallback: class `---@overload` / declared signatures.
fn call_return_type(
    model: &SalsaSemanticModel<'_>,
    call: &emmylua_parser::LuaCallExpr,
) -> Option<LuaType> {
    let prefix = call.get_prefix_expr()?;
    let emmylua_parser::LuaExpr::NameExpr(name_expr) = prefix else {
        return None;
    };
    let name = name_expr.get_name_text()?;

    // The variable name may differ from the type name (`local expect` has doc type Expect).
    let decl_ty = model
        .resolve_name(name_expr.get_position())
        .and_then(|decl| model.type_of_decl(&decl));
    let type_def = decl_ty
        .as_ref()
        .and_then(|ty| match ty {
            LuaType::Ref(id) | LuaType::Def(id) => model.type_def_of(id),
            _ => None,
        })
        .or_else(|| model.resolve_type_def(&name));

    // `---@class Expect ---@overload fun<T>(actual:T): Assertion<T>`.
    if let Some(def) = type_def {
        for overload in &def.call_overloads {
            if let LuaType::DocFunction(func) = model.doc_type_lua_in(def.file_id, *overload, &[])
                && let LuaType::Generic(generic) = func.get_ret()
            {
                return Some(LuaType::Ref(generic.get_base_type_id().clone()));
            }
        }
    }

    // Ordinary function signature.
    if let Some(decl) = model.resolve_name(name_expr.get_position()) {
        return model
            .type_of_decl_signature(&decl)
            .map(|func| func.get_ret().clone());
    }
    None
}
