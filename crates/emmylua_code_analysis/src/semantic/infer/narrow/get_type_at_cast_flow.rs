use emmylua_parser::{
    BinaryOperator, LuaAstNode, LuaCallExpr, LuaChunk, LuaDocOpType, LuaDocTagCast, LuaExpr,
};

use crate::{
    DbIndex, FileId, FlowId, FlowNode, FlowNodeKind, FlowTree, InFiled, InferFailReason,
    LuaInferCache, LuaType, LuaTypeOwner, TypeOps,
    semantic::infer::{
        VarRefId,
        narrow::{
            ResultTypeOrContinue, condition_flow::InferConditionFlow, get_single_antecedent,
            get_type_at_flow::get_type_at_flow, var_ref_id::get_var_expr_var_ref_id,
        },
    },
};

pub fn get_type_at_cast_flow(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    tag_cast: LuaDocTagCast,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    match tag_cast.get_key_expr() {
        Some(expr) => {
            get_type_at_cast_expr(db, tree, cache, root, var_ref_id, flow_node, tag_cast, expr)
        }
        None => get_type_at_inline_cast(db, tree, cache, root, var_ref_id, flow_node, tag_cast),
    }
}

#[allow(clippy::too_many_arguments)]
fn get_type_at_cast_expr(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    tag_cast: LuaDocTagCast,
    key_expr: LuaExpr,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let Some(maybe_ref_id) = get_var_expr_var_ref_id(db, cache, key_expr) else {
        return Ok(ResultTypeOrContinue::Continue);
    };

    if maybe_ref_id != *var_ref_id {
        return Ok(ResultTypeOrContinue::Continue);
    }

    let antecedent_flow_id = get_single_antecedent(flow_node)?;
    let mut antecedent_type =
        get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
    for cast_op_type in tag_cast.get_op_types() {
        antecedent_type = cast_type(
            db,
            cache.get_file_id(),
            cast_op_type,
            antecedent_type,
            InferConditionFlow::TrueCondition,
        )?;
    }
    Ok(ResultTypeOrContinue::Result(antecedent_type))
}

fn get_type_at_inline_cast(
    db: &DbIndex,
    tree: &FlowTree,
    cache: &mut LuaInferCache,
    root: &LuaChunk,
    var_ref_id: &VarRefId,
    flow_node: &FlowNode,
    tag_cast: LuaDocTagCast,
) -> Result<ResultTypeOrContinue, InferFailReason> {
    let antecedent_flow_id = get_single_antecedent(flow_node)?;
    let mut antecedent_type =
        get_type_at_flow(db, tree, cache, root, var_ref_id, antecedent_flow_id)?;
    for cast_op_type in tag_cast.get_op_types() {
        antecedent_type = cast_type(
            db,
            cache.get_file_id(),
            cast_op_type,
            antecedent_type,
            InferConditionFlow::TrueCondition,
        )?;
    }
    Ok(ResultTypeOrContinue::Result(antecedent_type))
}

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

enum CastAction {
    Add,
    Remove,
    Force,
}

impl CastAction {
    fn get_negative(&self) -> Self {
        match self {
            CastAction::Add => CastAction::Remove,
            CastAction::Remove => CastAction::Add,
            CastAction::Force => CastAction::Remove,
        }
    }
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

    if condition_flow.is_false() {
        action = action.get_negative();
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
