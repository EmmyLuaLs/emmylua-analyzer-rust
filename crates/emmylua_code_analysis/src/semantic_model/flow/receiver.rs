//! Field types after an assignment to their receiver.

use emmylua_parser::{BinaryOperator, LuaAstNode, LuaExpr, LuaIndexExpr, LuaNameExpr, LuaVarExpr};
use rowan::TextRange;

use crate::semantic_db::def::{AccessPath, MemberSource, SemanticId};
use crate::semantic_db::flow::{FlowEffect, FlowId, FlowTree};
use crate::{LuaMemberKey, LuaType};

use super::super::{member, type_eval};
use super::{
    MemberTrace, SemanticModel, assignment_antecedent_reachable, assignment_target_index, cache,
    field_rhs_flow_type, flow_assign_stat, is_unasserted_self_assignment, literal_type,
    member_assignment_value, unparenthesized,
};

pub(super) fn receiver_assignment_keeps_value(
    model: &SemanticModel,
    decl: &SemanticId,
    value: &LuaExpr,
) -> bool {
    selected_receiver_name(model, value.clone())
        .is_some_and(|name| is_unasserted_self_assignment(model, decl, value, &name))
}

/// Follow short-circuit operands when literals or local types prove the choice.
fn selected_receiver_name(model: &SemanticModel, mut operand: LuaExpr) -> Option<LuaNameExpr> {
    let mut pending = Vec::new();
    'select: loop {
        operand = unparenthesized(operand)?;
        if let LuaExpr::BinaryExpr(binary) = operand {
            let op = binary.get_op_token()?.get_op();
            if !matches!(op, BinaryOperator::OpOr | BinaryOperator::OpAnd) {
                return None;
            }
            let (left, right) = binary.get_exprs()?;
            pending.push((op, right));
            operand = left;
            continue;
        }
        if !pending.is_empty() {
            let ty = match &operand {
                LuaExpr::LiteralExpr(literal) => literal_type(literal)?,
                LuaExpr::NameExpr(name) => {
                    let decl = model.resolve_name(name.get_position())?;
                    model.type_of_decl_at(&decl, name.get_position())
                }
                _ => return None,
            };
            let ty = type_eval::expand_alias_branches(model, &ty)?;
            let truthy = ty.is_always_truthy();
            if ty.is_never() || (!truthy && !ty.is_always_falsy()) {
                return None;
            }
            while let Some((op, right)) = pending.pop() {
                if truthy != (op == BinaryOperator::OpOr) {
                    operand = right;
                    continue 'select;
                }
            }
        }
        return match operand {
            LuaExpr::NameExpr(name) => Some(name),
            _ => None,
        };
    }
}

pub(super) fn member_type_after_receiver_assignment(
    model: &SemanticModel,
    access: Option<&LuaIndexExpr>,
    tree: &FlowTree,
    flow_id: FlowId,
    effects: &[FlowEffect],
) -> MemberTrace {
    let Some(path) = access
        .and_then(|access| model.member_use(access.get_syntax_id()))
        .and_then(|usage| usage.access_path.as_ref())
    else {
        return Ok(None);
    };
    let Some(rhs) = effects.iter().find_map(|effect| match effect {
        FlowEffect::AssignDecl { decl, value_syntax } if *decl == path.root => Some(*value_syntax),
        _ => None,
    }) else {
        return Ok(None);
    };
    if tree.has_condition() && !assignment_antecedent_reachable(model, tree, flow_id) {
        return Ok(Some(LuaType::Never));
    }
    if let Some(ty) = field_written_on_replacement(model, tree, flow_id, path, effects)? {
        return Ok(Some(ty));
    }
    let Some(receiver) = receiver_replacement_type(model, tree, flow_id, &path.root)? else {
        return Ok(None);
    };
    if let Some(ty) = replacement_field_type(model, receiver, &path.keys, Some(rhs.get_range()))? {
        return Ok(Some(ty));
    }
    let declared = (|| {
        model
            .file_facts()?
            .decl_by_id(&path.root)?
            .doc_type_syntax?;
        model.type_of_decl(&path.root)
    })();
    match declared {
        Some(declared) => replacement_field_type(model, declared, &path.keys, None),
        None => Ok(None),
    }
}

/// A field target on the RHS local still writes that table in a parallel assignment.
fn field_written_on_replacement(
    model: &SemanticModel,
    tree: &FlowTree,
    flow_id: FlowId,
    path: &AccessPath,
    effects: &[FlowEffect],
) -> MemberTrace {
    let source = (|| {
        if !effects
            .iter()
            .any(|effect| matches!(effect, FlowEffect::AssignMember { .. }))
        {
            return None;
        }
        let assign = flow_assign_stat(model, tree, flow_id)?;
        let (vars, values) = assign.get_var_and_expr_list();
        if vars
            .iter()
            .filter(|var| matches!(var, LuaVarExpr::IndexExpr(_)))
            .take(2)
            .count()
            != 1
        {
            return None;
        }
        let slot = assignment_target_index(model, &vars, &path.root)?;
        let LuaExpr::NameExpr(name) = unparenthesized(values.get(slot)?.clone())? else {
            return None;
        };
        let decl = model.resolve_name(name.get_position())?;
        (decl != path.root).then_some(decl)
    })();
    let (Some(source), Some(facts)) = (source, model.file_facts()) else {
        return Ok(None);
    };
    for effect in effects {
        let FlowEffect::AssignMember {
            member,
            value_syntax,
            result_slot,
            ..
        } = effect
        else {
            continue;
        };
        let Some(member) = facts.member_by_id(member) else {
            continue;
        };
        let MemberSource::Runtime { access } = member.source else {
            continue;
        };
        if facts
            .member_use(access)
            .and_then(|usage| usage.access_path.as_ref())
            .is_some_and(|target| {
                target.root == source && target.keys == path.keys && target.is_exact()
            })
        {
            let ty = member_assignment_value(model, *value_syntax)?;
            return Ok(Some(
                ty.get_result_slot_type(*result_slot)
                    .unwrap_or(LuaType::Nil),
            ));
        }
    }
    Ok(None)
}

/// The replacement value is shared by all field reads after this assignment.
fn receiver_replacement_type(
    model: &SemanticModel,
    tree: &FlowTree,
    flow_id: FlowId,
    decl: &SemanticId,
) -> MemberTrace {
    let key = (model.file_id(), flow_id, decl.clone());
    {
        let cache = model.cache.borrow();
        if let Some(entry) = cache.receiver_replacement.get(&key) {
            return Ok(entry.get(&cache).cloned().unwrap_or(Some(LuaType::Unknown)));
        }
    }
    let Some(assign) = flow_assign_stat(model, tree, flow_id) else {
        return Ok(None);
    };
    let (vars, values) = assign.get_var_and_expr_list();
    let Some(slot) = assignment_target_index(model, &vars, decl) else {
        return Ok(None);
    };
    let Some(value) = values.get(slot).or_else(|| values.last()) else {
        return Ok(None);
    };
    let result_slot = slot.saturating_sub(values.len() - 1);

    // A simultaneous field write may mutate an existing replacement through
    // another name. A table constructed here is distinct from those targets.
    let may_write_replacement = vars
        .iter()
        .any(|var| matches!(var, LuaVarExpr::IndexExpr(_)))
        && !matches!(unparenthesized(value.clone()), Some(LuaExpr::TableExpr(_)));

    let _query = cache::QueryCacheScope::new(&model.cache);
    model
        .cache
        .borrow_mut()
        .receiver_replacement
        .insert(key.clone(), cache::CacheEntry::InProgress);
    let revision = model.cache.borrow().revision();
    let replacement = if may_write_replacement
        || (result_slot == 0 && receiver_assignment_keeps_value(model, decl, value))
    {
        Ok(None)
    } else {
        member_assignment_value(model, value.get_syntax_id())
            .map(|ty| Some(ty.get_result_slot_type(result_slot).unwrap_or(LuaType::Nil)))
    };
    let mut cache = model.cache.borrow_mut();
    match &replacement {
        Ok(ty) => {
            let entry = cache::CacheEntry::finish(ty.clone(), cache.is_complete(revision));
            cache.receiver_replacement.insert(key, entry);
        }
        Err(_) => {
            cache.receiver_replacement.remove(&key);
        }
    }
    replacement
}

fn replacement_field_type(
    model: &SemanticModel,
    receiver: LuaType,
    keys: &[LuaMemberKey],
    rhs: Option<TextRange>,
) -> MemberTrace {
    let mut receivers = vec![receiver];
    for key in keys {
        let mut fields = Vec::new();
        // Member lookup can omit union branches that lack the key. A
        // replacement only proves a field type if every branch has that field.
        while let Some(receiver) = receivers.pop() {
            let Some(receiver) = type_eval::expand_alias_head(model, &receiver) else {
                return Ok(None);
            };
            match receiver {
                LuaType::Union(union) => receivers.extend(union.into_vec()),
                LuaType::MultiLineUnion(union) => receivers.extend(union.iter().cloned()),
                LuaType::Any | LuaType::Unknown => return Ok(None),
                receiver => {
                    let Some(field) = replacement_field_value(model, &receiver, key, rhs)? else {
                        return Ok(None);
                    };
                    fields.push(field);
                }
            }
        }
        receivers = fields;
    }
    let ty = LuaType::from_vec(receivers);
    Ok((!matches!(ty, LuaType::Any | LuaType::Unknown)).then_some(ty))
}

fn replacement_field_value(
    model: &SemanticModel,
    receiver: &LuaType,
    key: &LuaMemberKey,
    rhs: Option<TextRange>,
) -> MemberTrace {
    let Some(info) = member::member_info(model, receiver, key) else {
        return Ok(None);
    };
    // If this table was constructed in the RHS, its explicit field initializer
    // can itself read an earlier write. Use the same evaluator as field assignments.
    if let LuaType::TableConst(table) = receiver
        && table.file_id == model.file_id()
        && rhs.is_some_and(|rhs| rhs.contains_range(table.value))
        && let Some(id) = &info.id
        && let Some(member) = model.file_facts().and_then(|facts| facts.member_by_id(id))
        && model.is_initializer_table_field(member)
        && member.doc_type_syntax.is_none()
        && let Some(value) = member.value_syntax
        && let Some(ty) = field_rhs_flow_type(model, value)?
    {
        return Ok(ty.get_result_slot_type(0));
    }
    Ok(Some(info.typ))
}
