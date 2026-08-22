mod array;
mod declared;
mod generic;
mod object_type;
mod table_const;
mod table_generic;
mod tuple;

use crate::LuaType;

use super::{
    mismatch::TypeMismatch,
    relation::{IntersectionState, Relater, RelationResult},
};
use array::relate_array_source;
pub(in crate::semantic::type_check) use array::relate_array_to_array;
use declared::{
    relate_base_source_to_declared_target, relate_declared_source,
    relate_structural_source_to_declared_target,
};
use generic::relate_generic_source;
use object_type::relate_object_source;
pub(in crate::semantic::type_check) use object_type::{
    relate_object_members, relate_target_intersection_index_obligations,
    relate_to_declared_target_members,
};
use table_const::{relate_table_const_source, relate_to_table_const_target};
use table_generic::relate_table_generic_source;
use tuple::{relate_keyed_source_to_tuple, relate_tuple_source};

pub(super) fn relate_structured(
    relater: &mut Relater,
    source: &LuaType,
    target: &LuaType,
    intersection_state: IntersectionState,
) -> Option<RelationResult> {
    match source {
        LuaType::Table => match target {
            LuaType::Table
            | LuaType::Object(_)
            | LuaType::Tuple(_)
            | LuaType::Array(_)
            | LuaType::TableGeneric(_)
            | LuaType::TableConst(_) => {
                relater.note_progress();
                Some(Ok(()))
            }
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
                let target_decl = match target {
                    LuaType::Ref(target_id) | LuaType::Def(target_id) => {
                        relater.db().get_type_index().get_type_decl(target_id)
                    }
                    LuaType::Generic(target_generic) => relater
                        .db()
                        .get_type_index()
                        .get_type_decl(target_generic.get_base_type_id_ref()),
                    _ => None,
                };
                let Some(target_decl) = target_decl else {
                    return Some(relater.unrelated(|| TypeMismatch::incompatible(source, target)));
                };

                if target_decl.is_alias() || target_decl.is_enum() {
                    Some(relate_structural_source_to_declared_target(
                        relater,
                        source,
                        target,
                        intersection_state,
                    ))
                } else {
                    relater.note_progress();
                    Some(Ok(()))
                }
            }
            _ => None,
        },
        LuaType::Array(source_array) => {
            relate_array_source(relater, source, source_array, target, intersection_state)
        }
        LuaType::Tuple(source_tuple) => {
            relate_tuple_source(relater, source, source_tuple, target, intersection_state)
        }
        LuaType::TableConst(source_range) => {
            relate_table_const_source(relater, source, source_range, target, intersection_state)
        }
        LuaType::Object(source_object) => {
            relate_object_source(relater, source, source_object, target, intersection_state)
        }
        LuaType::Ref(decl_id) | LuaType::Def(decl_id) => {
            relate_declared_source(relater, source, decl_id, target, intersection_state)
        }
        LuaType::Generic(generic_type) => {
            relate_generic_source(relater, source, generic_type, target, intersection_state)
        }
        LuaType::TableGeneric(source_params) => {
            relate_table_generic_source(relater, source, source_params, target, intersection_state)
        }
        LuaType::Intersection(_) => match target {
            LuaType::Tuple(target_tuple) => Some(relate_keyed_source_to_tuple(
                relater,
                source,
                target_tuple,
                intersection_state,
            )),
            LuaType::Object(target_object) => Some(relate_object_members(
                relater,
                source,
                target,
                target_object,
                intersection_state,
            )),
            LuaType::TableConst(target_range) => Some(relate_to_table_const_target(
                relater,
                source,
                target,
                target_range,
                intersection_state,
            )),
            LuaType::Ref(_) | LuaType::Def(_) | LuaType::Generic(_) => {
                Some(relate_structural_source_to_declared_target(
                    relater,
                    source,
                    target,
                    intersection_state,
                ))
            }
            _ => None,
        },
        _ => relate_base_source_to_declared_target(relater, source, target, intersection_state),
    }
}
