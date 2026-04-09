use emmylua_parser::{BinaryOperator, LuaAstNode, LuaCallExpr, LuaChunk, LuaDocOpType};

use crate::{
    DbIndex, FileId, FlowId, FlowNodeKind, FlowTree, InFiled, InferFailReason, LuaInferCache,
    LuaType, LuaTypeOwner, TypeOps, semantic::infer::narrow::condition_flow::InferConditionFlow,
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
