use crate::LuaType;

pub trait TypeVisitTrait {
    fn visit_type<F>(&self, f: &mut F)
    where
        F: FnMut(&LuaType);
}
