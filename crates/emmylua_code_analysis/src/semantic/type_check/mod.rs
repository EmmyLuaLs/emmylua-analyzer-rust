mod complex_type;
mod func_type;
mod generic_type;
mod ref_type;
mod simple_type;
mod sub_type;
mod test;
mod type_check_fail_reason;
mod type_check_guard;

use complex_type::check_complex_type_compact;
use func_type::{check_doc_func_type_compact, check_sig_type_compact};
use generic_type::check_generic_type_compact;
use ref_type::check_ref_type_compact;
use simple_type::check_simple_type_compact;
pub use type_check_fail_reason::TypeCheckFailReason;
use type_check_guard::TypeCheckGuard;

use crate::{
    db_index::{DbIndex, LuaType},
    TypeOps,
};
pub use sub_type::is_sub_type_of;
pub type TypeCheckResult = Result<(), TypeCheckFailReason>;

pub fn check_type_compact(
    db: &DbIndex,
    source: &LuaType,
    compact_type: &LuaType,
) -> TypeCheckResult {
    check_general_type_compact(db, source, compact_type, TypeCheckGuard::new())
}

fn check_general_type_compact(
    db: &DbIndex,
    source: &LuaType,
    compact_type: &LuaType,
    check_guard: TypeCheckGuard,
) -> TypeCheckResult {
    if is_like_any(compact_type) {
        return Ok(());
    }

    if let Some(origin_type) = escape_type(db, compact_type) {
        return check_general_type_compact(db, source, &origin_type, check_guard.next_level()?);
    }

    match source {
        LuaType::Unknown | LuaType::Any => Ok(()),
        // simple type
        LuaType::Nil
        | LuaType::Table
        | LuaType::Userdata
        | LuaType::Function
        | LuaType::Thread
        | LuaType::Boolean
        | LuaType::String
        | LuaType::Integer
        | LuaType::Number
        | LuaType::Io
        | LuaType::Global
        | LuaType::BooleanConst(_)
        | LuaType::StringConst(_)
        | LuaType::IntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::TableConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::TplRef(_)
        | LuaType::StrTplRef(_)
        | LuaType::Namespace(_)
        | LuaType::Variadic(_) => check_simple_type_compact(db, source, compact_type, check_guard),

        // type ref
        LuaType::Ref(type_decl_id) => {
            check_ref_type_compact(db, type_decl_id, compact_type, check_guard)
        }
        LuaType::Def(type_decl_id) => {
            check_ref_type_compact(db, type_decl_id, compact_type, check_guard)
        }
        // invaliad source type
        // LuaType::Module(arc_intern) => todo!(),

        // function type
        LuaType::DocFunction(doc_func) => {
            check_doc_func_type_compact(db, doc_func, compact_type, check_guard)
        }
        // signature type
        LuaType::Signature(sig_id) => check_sig_type_compact(db, sig_id, compact_type, check_guard),

        // complex type
        LuaType::Array(_)
        | LuaType::Nullable(_)
        | LuaType::Tuple(_)
        | LuaType::Object(_)
        | LuaType::Union(_)
        | LuaType::Intersection(_)
        | LuaType::TableGeneric(_)
        | LuaType::MultiLineUnion(_) => {
            check_complex_type_compact(db, source, compact_type, check_guard)
        }

        // need think how to do that
        LuaType::Call(_) => Ok(()),

        // generic type
        LuaType::Generic(generic) => {
            check_generic_type_compact(db, generic, compact_type, check_guard)
        }
        // invalid source type
        // LuaType::MuliReturn(_) |
        // LuaType::MemberPathExist(_) |
        // LuaType::Instance(_) |
        _ => Err(TypeCheckFailReason::TypeNotMatch),
    }
}

fn is_like_any(ty: &LuaType) -> bool {
    matches!(
        ty,
        LuaType::Any | LuaType::Unknown | LuaType::TplRef(_) | LuaType::StrTplRef(_)
    )
}

fn escape_type(db: &DbIndex, typ: &LuaType) -> Option<LuaType> {
    match typ {
        LuaType::Ref(type_id) => {
            let type_decl = db.get_type_index().get_type_decl(type_id)?;
            if type_decl.is_alias() {
                if let Some(origin_type) = type_decl.get_alias_origin(db, None) {
                    return Some(origin_type.clone());
                }
            }
        }
        // todo donot escape
        LuaType::Instance(inst) => {
            let base = inst.get_base();
            return Some(base.clone());
        }
        LuaType::MemberPathExist(member_path) => {
            let base = member_path.get_origin();
            return Some(TypeOps::Remove.apply(base, &LuaType::Nil));
        }
        LuaType::MultiLineUnion(multi_union) => {
            let union = multi_union.to_union();
            return Some(union);
        }
        _ => {}
    }

    None
}
