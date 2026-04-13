use hashbrown::HashSet;

use super::{LuaType, LuaTypeNode, LuaUnionType, VariadicType};

#[allow(unused)]
impl LuaType {
    pub fn is_unknown(&self) -> bool {
        matches!(self, LuaType::Unknown)
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, LuaType::Nil)
    }

    pub fn is_never(&self) -> bool {
        matches!(self, LuaType::Never)
    }

    pub fn is_table(&self) -> bool {
        matches!(
            self,
            LuaType::Table
                | LuaType::TableGeneric(_)
                | LuaType::TableConst(_)
                | LuaType::Global
                | LuaType::Tuple(_)
                | LuaType::Array(_)
        )
    }

    pub fn is_userdata(&self) -> bool {
        matches!(self, LuaType::Userdata)
    }

    pub fn is_thread(&self) -> bool {
        matches!(self, LuaType::Thread)
    }

    pub fn is_boolean(&self) -> bool {
        matches!(
            self,
            LuaType::BooleanConst(_) | LuaType::Boolean | LuaType::DocBooleanConst(_)
        )
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self,
            LuaType::StringConst(_)
                | LuaType::String
                | LuaType::DocStringConst(_)
                | LuaType::Language(_)
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            LuaType::IntegerConst(_) | LuaType::Integer | LuaType::DocIntegerConst(_)
        )
    }

    pub fn is_number(&self) -> bool {
        matches!(
            self,
            LuaType::Number | LuaType::Integer | LuaType::IntegerConst(_) | LuaType::FloatConst(_)
        )
    }

    pub fn is_io(&self) -> bool {
        matches!(self, LuaType::Io)
    }

    pub fn is_ref(&self) -> bool {
        matches!(self, LuaType::Ref(_))
    }

    pub fn is_def(&self) -> bool {
        matches!(self, LuaType::Def(_))
    }

    pub fn is_custom_type(&self) -> bool {
        matches!(self, LuaType::Ref(_) | LuaType::Def(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, LuaType::Array(_))
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            LuaType::Nil => true,
            LuaType::Union(u) => u.is_nullable(),
            _ => false,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            LuaType::Nil | LuaType::Any | LuaType::Unknown => true,
            LuaType::Union(u) => u.is_optional(),
            LuaType::Variadic(_) => true,
            _ => false,
        }
    }

    pub fn is_always_truthy(&self) -> bool {
        match self {
            LuaType::Nil | LuaType::Boolean | LuaType::Any | LuaType::Unknown => false,
            LuaType::BooleanConst(boolean) | LuaType::DocBooleanConst(boolean) => *boolean,
            LuaType::Union(u) => u.is_always_truthy(),
            LuaType::TypeGuard(_) => false,
            _ => true,
        }
    }

    pub fn is_always_falsy(&self) -> bool {
        match self {
            LuaType::Nil | LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false) => true,
            LuaType::Union(u) => u.is_always_falsy(),
            LuaType::TypeGuard(_) => false,
            _ => false,
        }
    }

    pub fn is_tuple(&self) -> bool {
        matches!(self, LuaType::Tuple(_))
    }

    pub fn is_function(&self) -> bool {
        matches!(
            self,
            LuaType::DocFunction(_) | LuaType::Function | LuaType::Signature(_)
        )
    }

    pub fn is_signature(&self) -> bool {
        matches!(self, LuaType::Signature(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, LuaType::Object(_))
    }

    pub fn is_union(&self) -> bool {
        matches!(self, LuaType::Union(_))
    }

    pub fn is_intersection(&self) -> bool {
        matches!(self, LuaType::Intersection(_))
    }

    pub fn is_call(&self) -> bool {
        matches!(self, LuaType::Call(_))
    }

    pub fn is_generic(&self) -> bool {
        matches!(self, LuaType::Generic(_) | LuaType::TableGeneric(_))
    }

    pub fn is_table_generic(&self) -> bool {
        matches!(self, LuaType::TableGeneric(_))
    }

    pub fn is_class_tpl(&self) -> bool {
        matches!(self, LuaType::TplRef(_))
    }

    pub fn is_str_tpl_ref(&self) -> bool {
        matches!(self, LuaType::StrTplRef(_))
    }

    pub fn is_tpl(&self) -> bool {
        matches!(self, LuaType::TplRef(_) | LuaType::StrTplRef(_))
    }

    pub fn is_self_infer(&self) -> bool {
        matches!(self, LuaType::SelfInfer)
    }

    pub fn is_any(&self) -> bool {
        matches!(self, LuaType::Any)
    }

    pub fn is_const(&self) -> bool {
        matches!(
            self,
            LuaType::BooleanConst(_)
                | LuaType::StringConst(_)
                | LuaType::IntegerConst(_)
                | LuaType::FloatConst(_)
                | LuaType::TableConst(_)
                | LuaType::DocStringConst(_)
                | LuaType::DocIntegerConst(_)
        )
    }

    pub fn is_multi_return(&self) -> bool {
        matches!(self, LuaType::Variadic(_))
    }

    pub fn contain_multi_return(&self) -> bool {
        match self {
            LuaType::Variadic(_) => true,
            LuaType::Union(union) => union.into_vec().iter().any(LuaType::contain_multi_return),
            _ => false,
        }
    }

    pub fn get_result_slot_type(&self, idx: usize) -> Option<LuaType> {
        match self {
            LuaType::Variadic(variadic) => match variadic.as_ref() {
                VariadicType::Base(base) => Some(base.clone()),
                VariadicType::Multi(types) => {
                    let last_idx = types.len().checked_sub(1)?;
                    if idx < last_idx {
                        return types[idx].get_result_slot_type(0);
                    }

                    let last_type = types.get(last_idx)?;
                    let offset = idx - last_idx;
                    last_type.get_result_slot_type(offset)
                }
            },
            LuaType::Union(union) => {
                let slot_types = union
                    .into_vec()
                    .into_iter()
                    .map(|ty| ty.get_result_slot_type(idx))
                    .collect::<Vec<_>>();
                if !slot_types.iter().any(|ty| ty.is_some()) {
                    return None;
                }

                Some(LuaType::from_vec(
                    slot_types
                        .into_iter()
                        .map(|ty| ty.unwrap_or(LuaType::Nil))
                        .collect(),
                ))
            }
            _ if idx == 0 => Some(self.clone()),
            _ => None,
        }
    }

    pub fn is_global(&self) -> bool {
        matches!(self, LuaType::Global)
    }

    pub fn contain_tpl(&self) -> bool {
        let mut stack = vec![self];
        while let Some(ty) = stack.pop() {
            match ty {
                LuaType::TplRef(_)
                | LuaType::StrTplRef(_)
                | LuaType::ConstTplRef(_)
                | LuaType::SelfInfer
                | LuaType::Mapped(_) => return true,
                _ => ty.push_direct_children(&mut stack),
            }
        }

        false
    }

    pub fn is_namespace(&self) -> bool {
        matches!(self, LuaType::Namespace(_))
    }

    pub fn is_variadic(&self) -> bool {
        matches!(self, LuaType::Variadic(_))
    }

    pub fn is_member_owner(&self) -> bool {
        matches!(self, LuaType::Ref(_) | LuaType::TableConst(_))
    }

    pub fn is_type_guard(&self) -> bool {
        matches!(self, LuaType::TypeGuard(_))
    }

    pub fn is_multi_line_union(&self) -> bool {
        matches!(self, LuaType::MultiLineUnion(_))
    }

    pub fn from_vec(types: Vec<LuaType>) -> Self {
        match types.len() {
            0 => LuaType::Nil,
            1 => types[0].clone(),
            _ => {
                let mut result_types = Vec::new();
                let mut hash_set = HashSet::new();
                for typ in types {
                    match typ {
                        LuaType::Union(u) => {
                            for t in u.into_vec() {
                                if hash_set.insert(t.clone()) {
                                    result_types.push(t);
                                }
                            }
                        }
                        _ => {
                            if hash_set.insert(typ.clone()) {
                                result_types.push(typ);
                            }
                        }
                    }
                }

                match result_types.len() {
                    0 => LuaType::Nil,
                    1 => result_types[0].clone(),
                    _ => LuaType::Union(LuaUnionType::from_vec(result_types).into()),
                }
            }
        }
    }

    pub fn is_module_ref(&self) -> bool {
        matches!(self, LuaType::ModuleRef(_))
    }
}
