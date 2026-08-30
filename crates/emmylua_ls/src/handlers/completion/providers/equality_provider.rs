//! Equality context completion: `x == <??>` offers candidates based on the left operand / assignment target type.

use emmylua_code_analysis::LuaType;
use emmylua_parser::{
    BinaryOperator, LuaAst, LuaAstNode, LuaAstToken, LuaLiteralExpr, LuaTokenKind,
};

use crate::handlers::completion::completion_builder::CompletionBuilder;

use super::{CompletionProvider, ProviderDecision, function_provider::dispatch_type};

pub struct EqualityProvider;

impl CompletionProvider for EqualityProvider {
    fn name(&self) -> &'static str {
        "equality"
    }

    fn supports(&self, builder: &CompletionBuilder) -> bool {
        supports_provider(builder)
    }

    fn complete(&self, builder: &mut CompletionBuilder) -> ProviderDecision {
        complete_provider(builder).unwrap_or(ProviderDecision::NoMatch)
    }
}

fn complete_provider(builder: &mut CompletionBuilder) -> Option<ProviderDecision> {
    if builder.is_cancelled() {
        return None;
    }

    let types = get_token_should_type(builder)?;
    let mut should_stop = false;
    for typ in types {
        if dispatch_type(builder, &typ) == ProviderDecision::Stop {
            should_stop = true;
            break;
        }
    }

    if should_stop || !builder.is_invoked() {
        return Some(ProviderDecision::Stop);
    }
    Some(ProviderDecision::Continue)
}

fn supports_provider(builder: &CompletionBuilder) -> bool {
    let token = builder.trigger_token.clone();
    let Some(mut parent_node) = token.parent() else {
        return false;
    };
    if let Some(prev_parent) = token.prev_token().and_then(|prev| prev.parent()) {
        parent_node = prev_parent;
    } else if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
        let Some(next_parent) = parent_node.parent() else {
            return false;
        };
        parent_node = next_parent;
    }
    let ok = matches!(
        LuaAst::cast(parent_node.clone()),
        Some(
            LuaAst::LuaBinaryExpr(_)
                | LuaAst::LuaLocalStat(_)
                | LuaAst::LuaAssignStat(_)
                | LuaAst::LuaTableExpr(_)
                | LuaAst::LuaTableField(_)
        )
    );
    ok
}

fn get_token_should_type(builder: &CompletionBuilder) -> Option<Vec<LuaType>> {
    let token = builder.trigger_token.clone();
    let mut parent_node = token.parent()?;
    if let Some(node) = token.prev_token()?.parent() {
        parent_node = node;
    } else if LuaLiteralExpr::can_cast(parent_node.kind().into()) {
        parent_node = parent_node.parent()?;
    }

    match LuaAst::cast(parent_node)? {
        LuaAst::LuaBinaryExpr(binary_expr) => {
            let op_token = binary_expr.get_op_token()?;
            let op = op_token.get_op();
            if op == BinaryOperator::OpEq || op == BinaryOperator::OpNe {
                let left = binary_expr.get_left_expr()?;
                let left_type = expr_should_type(builder, &left);
                if !matches!(left_type, LuaType::Unknown) {
                    return Some(vec![left_type]);
                }
            }
            None
        }
        LuaAst::LuaLocalStat(local_stat) => {
            let locals = local_stat.get_local_name_list().collect::<Vec<_>>();
            if locals.len() != 1 {
                return None;
            }
            let position = builder.trigger_token.text_range().start();
            let eq = local_stat.token_by_kind(LuaTokenKind::TkAssign)?;
            if position < eq.get_position() {
                return None;
            }
            let local = locals.first()?;
            let decl = builder
                .semantic_model
                .decl_by_offset(local.get_position())?;
            builder
                .semantic_model
                .type_of_decl(&decl)
                .filter(|typ| !contains_function(builder, typ))
                .map(|typ| vec![typ])
        }
        LuaAst::LuaAssignStat(assign_stat) => {
            let (vars, _) = assign_stat.get_var_and_expr_list();
            if vars.len() != 1 {
                return None;
            }
            let position = builder.trigger_token.text_range().start();
            let eq = assign_stat.token_by_kind(LuaTokenKind::TkAssign)?;
            if position < eq.get_position() {
                return None;
            }
            let var = vars.first()?;
            let var_type = builder
                .semantic_model
                .type_of_expr(var.to_expr().get_syntax_id());
            if !contains_function(builder, &var_type) && !matches!(var_type, LuaType::Unknown) {
                return Some(vec![var_type]);
            }
            None
        }
        LuaAst::LuaTableExpr(table_expr) => {
            let table_type = builder
                .semantic_model
                .type_of_expr(table_expr.get_syntax_id());
            if let LuaType::Array(array) = table_type {
                return Some(vec![array.get_base().clone()]);
            }
            None
        }
        _ => None,
    }
}

fn expr_should_type(builder: &CompletionBuilder, expr: &emmylua_parser::LuaExpr) -> LuaType {
    let ty = builder.semantic_model.type_of_expr(expr.get_syntax_id());
    if !matches!(ty, LuaType::Unknown) {
        return ty;
    }

    // `type(a) == ...`: when VM inference fails for the doc return, use the declared signature's return type.
    let emmylua_parser::LuaExpr::CallExpr(call) = expr else {
        return LuaType::Unknown;
    };
    let Some(prefix) = call.get_prefix_expr() else {
        return LuaType::Unknown;
    };
    let emmylua_parser::LuaExpr::NameExpr(name) = prefix else {
        return LuaType::Unknown;
    };
    let Some(decl) = builder.semantic_model.resolve_name(name.get_position()) else {
        return LuaType::Unknown;
    };
    let ret = if let emmylua_code_analysis::SemanticId::Decl(key) = &decl
        && let Some(facts) = builder.semantic_model.file_facts_of(key.file_id)
        && let Some(decl_info) = facts.decl_by_id(&decl)
        && let Some(closure) = decl_info.value_expr_syntax
        && let Some(signature) = facts.signature_by_closure(closure)
        && let Some(docs) = signature.docs.as_ref()
    {
        docs.returns
            .first()
            .or_else(|| docs.return_overloads.first().map(|(_, syntax)| syntax))
            .map(|syntax| {
                builder
                    .semantic_model
                    .doc_type_lua_in(key.file_id, *syntax, &[])
            })
            .unwrap_or(LuaType::Unknown)
    } else {
        builder
            .semantic_model
            .type_of_decl_signature(&decl)
            .map(|func| func.get_ret().clone())
            .unwrap_or(LuaType::Unknown)
    };
    ret
}

fn contains_function(builder: &CompletionBuilder, typ: &LuaType) -> bool {
    match typ {
        LuaType::DocFunction(_) | LuaType::Function | LuaType::Signature(_) => true,
        LuaType::Union(union) => union.into_vec().iter().any(|component| {
            component.is_function()
                || matches!(component, LuaType::Ref(_) | LuaType::Def(_))
                    && builder
                        .semantic_model
                        .type_def_of(match component {
                            LuaType::Ref(id) | LuaType::Def(id) => id,
                            _ => return false,
                        })
                        .and_then(|def| builder.semantic_model.alias_target(&def))
                        .is_some_and(|target| contains_function(builder, &target))
        }),
        _ => false,
    }
}
