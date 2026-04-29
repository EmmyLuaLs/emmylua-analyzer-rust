use super::super::type_visit_trait::TypeVisitTrait;
use super::{
    LuaAliasCallType, LuaArrayType, LuaAttributeType, LuaConditionalType, LuaFunctionType,
    LuaGenericType, LuaIntersectionType, LuaMappedType, LuaMultiLineUnion, LuaObjectType,
    LuaTupleType, LuaType, LuaUnionType, VariadicType,
};

pub trait LuaTypeNode {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>);

    fn any_type<P>(&self, predicate: P) -> bool
    where
        P: FnMut(&LuaType) -> bool,
    {
        self.any_nested_type(predicate)
    }

    fn any_nested_type<P>(&self, mut predicate: P) -> bool
    where
        P: FnMut(&LuaType) -> bool,
    {
        let mut stack = Vec::new();
        self.push_direct_children(&mut stack);
        while let Some(ty) = stack.pop() {
            if predicate(ty) {
                return true;
            }
            ty.push_direct_children(&mut stack);
        }

        false
    }

    fn visit_nested_types<F>(&self, f: &mut F)
    where
        F: FnMut(&LuaType),
    {
        let mut stack = Vec::new();
        self.push_direct_children(&mut stack);
        while let Some(ty) = stack.pop() {
            f(ty);
            ty.push_direct_children(&mut stack);
        }
    }

    fn contain_tpl_children(&self) -> bool {
        self.any_nested_type(|ty| {
            matches!(
                ty,
                LuaType::TplRef(_)
                    | LuaType::StrTplRef(_)
                    | LuaType::ConstTplRef(_)
                    | LuaType::SelfInfer
                    | LuaType::Mapped(_)
            )
        })
    }

    fn contains_tpl_node(&self) -> bool {
        self.any_type(|ty| {
            matches!(
                ty,
                LuaType::TplRef(_)
                    | LuaType::StrTplRef(_)
                    | LuaType::ConstTplRef(_)
                    | LuaType::SelfInfer
                    | LuaType::Mapped(_)
            )
        })
    }
}

impl LuaTypeNode for LuaType {
    fn any_type<P>(&self, mut predicate: P) -> bool
    where
        P: FnMut(&LuaType) -> bool,
    {
        if predicate(self) {
            return true;
        }

        self.any_nested_type(predicate)
    }

    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        match self {
            LuaType::Array(base) => base.push_direct_children(stack),
            LuaType::Call(base) => base.push_direct_children(stack),
            LuaType::Tuple(base) => base.push_direct_children(stack),
            LuaType::DocFunction(base) => base.push_direct_children(stack),
            LuaType::Object(base) => base.push_direct_children(stack),
            LuaType::Union(base) => base.push_direct_children(stack),
            LuaType::Intersection(base) => base.push_direct_children(stack),
            LuaType::Generic(base) => base.push_direct_children(stack),
            LuaType::Variadic(multi) => multi.push_direct_children(stack),
            LuaType::TableGeneric(params) => {
                for param in params.iter().rev() {
                    stack.push(param);
                }
            }
            LuaType::MultiLineUnion(inner) => inner.push_direct_children(stack),
            LuaType::TypeGuard(inner) => stack.push(inner),
            LuaType::Conditional(inner) => inner.push_direct_children(stack),
            LuaType::Mapped(mapped) => mapped.push_direct_children(stack),
            _ => {}
        }
    }
}

impl TypeVisitTrait for LuaType {
    fn visit_type<F>(&self, f: &mut F)
    where
        F: FnMut(&LuaType),
    {
        let mut stack = vec![self];
        while let Some(ty) = stack.pop() {
            f(ty);
            ty.push_direct_children(&mut stack);
        }
    }
}

impl LuaTypeNode for LuaTupleType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        for ty in self.get_types().iter().rev() {
            stack.push(ty);
        }
    }
}

impl LuaTypeNode for LuaFunctionType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        stack.push(self.get_ret());
        for (_, ty) in self.get_params().iter().rev() {
            if let Some(ty) = ty {
                stack.push(ty);
            }
        }
    }
}

impl LuaTypeNode for LuaObjectType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        for (key, value) in self.get_index_access().iter().rev() {
            stack.push(value);
            stack.push(key);
        }
        for value in self.get_fields().values() {
            stack.push(value);
        }
    }
}

impl LuaTypeNode for LuaUnionType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        match self {
            LuaUnionType::Basic(_) => {}
            LuaUnionType::Nullable(ty) => stack.push(ty),
            LuaUnionType::Multi(types) => {
                for ty in types.iter().rev() {
                    stack.push(ty);
                }
            }
        }
    }
}

impl LuaTypeNode for LuaIntersectionType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        for ty in self.get_types().iter().rev() {
            stack.push(ty);
        }
    }
}

impl LuaTypeNode for LuaAliasCallType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        for ty in self.get_operands().iter().rev() {
            stack.push(ty);
        }
    }
}

impl LuaTypeNode for LuaGenericType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        for ty in self.get_params().iter().rev() {
            stack.push(ty);
        }
    }
}

impl LuaTypeNode for VariadicType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        match self {
            VariadicType::Multi(types) => {
                for ty in types.iter().rev() {
                    stack.push(ty);
                }
            }
            VariadicType::Base(ty) => stack.push(ty),
        }
    }
}

impl LuaTypeNode for super::LuaInstanceType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        stack.push(self.get_base());
    }
}

impl LuaTypeNode for LuaMultiLineUnion {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        for (ty, _) in self.get_unions().iter().rev() {
            stack.push(ty);
        }
    }
}

impl LuaTypeNode for LuaArrayType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        stack.push(self.get_base());
    }
}

impl LuaTypeNode for LuaAttributeType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        for (_, ty) in self.get_params().iter().rev() {
            if let Some(ty) = ty {
                stack.push(ty);
            }
        }
    }
}

impl LuaTypeNode for LuaConditionalType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        stack.push(self.get_false_type());
        stack.push(self.get_true_type());
        stack.push(self.get_extends_type());
        stack.push(self.get_checked_type());
    }
}

impl LuaTypeNode for LuaMappedType {
    fn push_direct_children<'a>(&'a self, stack: &mut Vec<&'a LuaType>) {
        stack.push(&self.value);
        if let Some(constraint) = self.param.1.type_constraint.as_ref() {
            stack.push(constraint);
        }
        if let Some(default_type) = self.param.1.default_type.as_ref() {
            stack.push(default_type);
        }
    }
}

macro_rules! impl_type_visit_trait {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TypeVisitTrait for $ty {
                fn visit_type<F>(&self, f: &mut F)
                where
                    F: FnMut(&LuaType),
                {
                    self.visit_nested_types(f);
                }
            }
        )*
    };
}

impl_type_visit_trait!(
    LuaTupleType,
    LuaFunctionType,
    LuaObjectType,
    LuaUnionType,
    LuaIntersectionType,
    LuaAliasCallType,
    LuaGenericType,
    VariadicType,
    super::LuaInstanceType,
    LuaMultiLineUnion,
    LuaArrayType,
    LuaAttributeType,
    LuaConditionalType,
    LuaMappedType,
);
