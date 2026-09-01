mod basic_union;
mod generic_param;
mod type_decl;
mod type_visit_trait;
mod types;

pub use basic_union::*;
pub use generic_param::GenericParam;
use internment::ArcIntern;
pub use type_decl::*;
pub use type_visit_trait::*;
pub use types::*;

/// Interned handle for a structural `LuaType`.
///
/// This is the type-key layer needed before high-level semantic queries can be
/// moved into salsa: it gives us a cheap, stable, equality-based handle that is
/// also a valid salsa value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct InternedLuaType(ArcIntern<LuaType>);

impl InternedLuaType {
    pub(crate) fn new(ty: LuaType) -> Self {
        Self(ArcIntern::new(ty))
    }

    pub(crate) fn as_ref(&self) -> &LuaType {
        self.0.as_ref()
    }

    pub(crate) fn into_inner(self) -> LuaType {
        (*self.0).clone()
    }
}

unsafe impl salsa::SalsaValue for InternedLuaType {}
