use emmylua_parser::{BinaryOperator, LuaAstNode, LuaCallExpr, LuaChunk, LuaDocOpType, LuaExpr};
use hashbrown::HashSet;

use crate::{
    DbIndex, FileId, FlowAntecedent, FlowId, FlowNodeKind, FlowTree, InFiled, InferFailReason,
    LuaInferCache, LuaType, LuaTypeOwner, TypeOps,
    semantic::infer::narrow::{
        VarRefId, condition_flow::InferConditionFlow, get_var_expr_var_ref_id,
    },
};

pub fn get_type_at_call_expr_inline_cast(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    tree: &FlowTree,
    call_expr: LuaCallExpr,
    flow_id: FlowId,
    mut return_type: LuaType,
) -> Option<LuaType> {
    let flow_node = tree.get_flow_node(flow_id)?;
    let FlowNodeKind::TagCast(tag_cast_ptr) = &flow_node.kind else {
        return None;
    };

    let root = LuaChunk::cast(call_expr.get_root())?;
    let tag_cast = tag_cast_ptr.to_node(&root)?;

    for cast_op_type in tag_cast.get_op_types() {
        return_type = match cast_type(
            db,
            cache.get_file_id(),
            cast_op_type,
            return_type,
            InferConditionFlow::TrueCondition,
        ) {
            Ok(typ) => typ,
            Err(_) => return None,
        };
    }

    Some(return_type)
}

pub(in crate::semantic) fn apply_assignment_target_casts(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    target_expr: LuaExpr,
    mut source_type: LuaType,
) -> LuaType {
    let file_id = cache.get_file_id();
    let Some(flow_tree) = db.get_flow_index().get_flow_tree(&file_id) else {
        return source_type;
    };
    if !flow_tree.has_tag_cast() {
        return source_type;
    }

    let target_syntax_id = target_expr.get_syntax_id();
    let target_root = target_expr.get_root();
    let Some(target_ref_id) = get_var_expr_var_ref_id(db, cache, target_expr) else {
        return source_type;
    };
    let Some(flow_id) = flow_tree.get_flow_id(target_syntax_id) else {
        return source_type;
    };
    let Some(root) = LuaChunk::cast(target_root) else {
        return source_type;
    };

    for cast_op_types in
        collect_assignment_target_casts(db, cache, flow_tree, &root, &target_ref_id, flow_id)
            .into_iter()
            .rev()
    {
        for cast_op_type in cast_op_types {
            source_type = cast_type(
                db,
                file_id,
                cast_op_type,
                source_type.clone(),
                InferConditionFlow::TrueCondition,
            )
            .unwrap_or(source_type);
        }
    }

    source_type
}

fn collect_assignment_target_casts(
    db: &DbIndex,
    cache: &mut LuaInferCache,
    flow_tree: &FlowTree,
    root: &LuaChunk,
    target_ref_id: &VarRefId,
    mut flow_id: FlowId,
) -> Vec<Vec<LuaDocOpType>> {
    // Assignment diagnostics compare against the target's declared/member contract,
    // not its current flow-narrowed read type. Reapply only explicit target casts
    // reachable on the current path; at joins, continue from the nearest common
    // predecessor so branch-local assertions do not leak out.
    let mut visited = HashSet::new();
    let mut cast_groups = Vec::new();
    while visited.insert(flow_id) {
        let Some(flow_node) = flow_tree.get_flow_node(flow_id) else {
            break;
        };
        if let FlowNodeKind::TagCast(cast_ptr) = &flow_node.kind
            && let Some(tag_cast) = cast_ptr.to_node(root)
            && let Some(key_expr) = tag_cast.get_key_expr()
            && get_var_expr_var_ref_id(db, cache, key_expr).as_ref() == Some(target_ref_id)
        {
            cast_groups.push(tag_cast.get_op_types().collect());
        }

        match flow_node.antecedent.as_ref() {
            Some(FlowAntecedent::Single(antecedent_flow_id)) => {
                flow_id = *antecedent_flow_id;
            }
            Some(FlowAntecedent::Multiple(multi_id)) => {
                let Some(branch_flow_ids) = flow_tree.get_multi_antecedents(*multi_id) else {
                    break;
                };
                let Some(common_flow_id) = flow_tree.get_nearest_common_antecedent(branch_flow_ids)
                else {
                    break;
                };
                flow_id = common_flow_id;
            }
            None => break,
        }
    }

    cast_groups
}

enum CastAction {
    Add,
    Remove,
    Force,
}

pub fn cast_type(
    db: &DbIndex,
    file_id: FileId,
    cast_op_type: LuaDocOpType,
    mut source_type: LuaType,
    condition_flow: InferConditionFlow,
) -> Result<LuaType, InferFailReason> {
    let mut action = match cast_op_type.get_op() {
        Some(op) => {
            if op.get_op() == BinaryOperator::OpAdd {
                CastAction::Add
            } else {
                CastAction::Remove
            }
        }
        None => CastAction::Force,
    };

    if matches!(condition_flow, InferConditionFlow::FalseCondition) {
        action = match action {
            CastAction::Add => CastAction::Remove,
            CastAction::Remove => CastAction::Add,
            CastAction::Force => CastAction::Remove,
        };
    }

    if cast_op_type.is_nullable() {
        match action {
            CastAction::Add => {
                source_type = TypeOps::Union.apply(db, &source_type, &LuaType::Nil);
            }
            CastAction::Remove => {
                source_type = TypeOps::Remove.apply(db, &source_type, &LuaType::Nil);
            }
            _ => {}
        }
    } else if let Some(doc_type) = cast_op_type.get_type() {
        let type_owner = LuaTypeOwner::SyntaxId(InFiled {
            file_id,
            value: doc_type.get_syntax_id(),
        });
        let typ = match db.get_type_index().get_type_cache(&type_owner) {
            Some(type_cache) => type_cache.as_type().clone(),
            None => return Ok(source_type),
        };
        match action {
            CastAction::Add => {
                source_type = TypeOps::Union.apply(db, &source_type, &typ);
            }
            CastAction::Remove => {
                source_type = TypeOps::Remove.apply(db, &source_type, &typ);
            }
            CastAction::Force => {
                source_type = typ;
            }
        }
    }

    Ok(source_type)
}
