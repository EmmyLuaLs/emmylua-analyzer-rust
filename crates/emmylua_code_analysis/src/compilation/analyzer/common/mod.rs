mod bind_type;
mod member;

pub use bind_type::{add_member, bind_type};
use emmylua_parser::LuaIndexExpr;
pub use member::*;

use crate::{DbIndex, GlobalId};

pub fn get_global_path(_: &DbIndex, _: &LuaIndexExpr) -> Option<GlobalId> {
    None
}
