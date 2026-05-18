use std::{ops::Deref, sync::Arc};

use hashbrown::{HashMap, HashSet};
use rowan::TextRange;

use crate::{
    DbIndex, GenericParam, InFiled, LuaArrayType, LuaConditionalType, LuaFunctionType,
    LuaGenericType, LuaMappedType, LuaMemberKey, LuaMemberOwner, LuaObjectType, LuaTupleType,
    LuaType, LuaUnionType, TypeOps, VariadicType,
};

pub(in crate::semantic::generic) fn is_primitive_or_literal_type(ty: &LuaType) -> bool {
    match ty {
        LuaType::String
        | LuaType::Number
        | LuaType::Integer
        | LuaType::Boolean
        | LuaType::StringConst(_)
        | LuaType::DocStringConst(_)
        | LuaType::IntegerConst(_)
        | LuaType::DocIntegerConst(_)
        | LuaType::FloatConst(_)
        | LuaType::BooleanConst(_)
        | LuaType::DocBooleanConst(_) => true,
        LuaType::Tuple(tuple) => tuple.get_types().iter().any(is_primitive_or_literal_type),
        LuaType::Union(union) => union.into_vec().iter().any(is_primitive_or_literal_type),
        LuaType::MultiLineUnion(union) => union
            .get_unions()
            .iter()
            .any(|(ty, _)| is_primitive_or_literal_type(ty)),
        LuaType::Variadic(variadic) => match variadic.deref() {
            VariadicType::Base(base) => is_primitive_or_literal_type(base),
            VariadicType::Multi(types) => types.iter().any(is_primitive_or_literal_type),
        },
        LuaType::Call(call) => call.get_operands().iter().any(is_primitive_or_literal_type),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WideningContext {
    Root,
    RootUnionMember,
    UnionMember,
    ObjectProperty,
    ArrayElement,
    TupleElement,
    VariadicElement,
}

const MAX_WIDENING_DEPTH: u16 = 100;

#[derive(Default)]
struct WideningGuard {
    depth: u16,
    active_table_ids: HashSet<InFiled<TextRange>>,
}

impl WideningGuard {
    fn enter_level(&mut self) -> bool {
        if self.depth >= MAX_WIDENING_DEPTH {
            return false;
        }
        self.depth += 1;
        true
    }

    fn leave_level(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn enter_table(&mut self, table_id: &InFiled<TextRange>) -> bool {
        self.active_table_ids.insert(table_id.clone())
    }

    fn leave_table(&mut self, table_id: &InFiled<TextRange>) {
        self.active_table_ids.remove(table_id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootPrimitiveBehavior {
    PreserveLiteral,
    WidenLiteral,
}

struct WideningTransformer<'db> {
    db: Option<&'db DbIndex>,
    root_primitive_behavior: RootPrimitiveBehavior,
    guard: WideningGuard,
}

impl<'db> WideningTransformer<'db> {
    fn new(db: Option<&'db DbIndex>, root_primitive_behavior: RootPrimitiveBehavior) -> Self {
        Self {
            db,
            root_primitive_behavior,
            guard: WideningGuard::default(),
        }
    }

    fn for_candidate_regularization(db: &'db DbIndex) -> Self {
        Self::new(Some(db), RootPrimitiveBehavior::PreserveLiteral)
    }

    fn for_candidate_widening(db: &'db DbIndex) -> Self {
        Self::new(Some(db), RootPrimitiveBehavior::WidenLiteral)
    }

    fn transform(&mut self, ty: LuaType, context: WideningContext) -> LuaType {
        if !self.guard.enter_level() {
            return self.fallback(ty, context);
        }

        let widened = match ty {
            LuaType::TableConst(table_id) => self.transform_table_const(table_id),
            LuaType::Array(array) => self.transform_array(array, context),
            LuaType::Tuple(tuple) => self.transform_tuple(tuple),
            LuaType::Object(object) => self.transform_object(object),
            LuaType::Union(union) => self.transform_union(union, context),
            LuaType::MultiLineUnion(multi) => self.transform_multi_line_union(multi, context),
            LuaType::Intersection(intersection) => {
                self.transform_intersection(intersection, context)
            }
            LuaType::Variadic(variadic) => self.transform_variadic(variadic),
            LuaType::Generic(generic) => self.transform_generic(generic),
            LuaType::TableGeneric(params) => self.transform_table_generic(params),
            LuaType::DocFunction(func) => self.transform_doc_function(func),
            LuaType::TypeGuard(type_guard) => self.transform_type_guard(type_guard),
            LuaType::Conditional(conditional) => self.transform_conditional(conditional),
            LuaType::Mapped(mapped) => self.transform_mapped(mapped),
            ty => self.transform_terminal(ty, context),
        };

        self.guard.leave_level();
        widened
    }

    fn fallback(&self, ty: LuaType, context: WideningContext) -> LuaType {
        match (self.db, ty) {
            (Some(_), LuaType::TableConst(_)) => LuaType::Table,
            (_, ty) => self.transform_terminal(ty, context),
        }
    }

    fn transform_table_const(&mut self, table_id: InFiled<TextRange>) -> LuaType {
        let Some(db) = self.db else {
            return LuaType::TableConst(table_id);
        };

        self.table_const_to_object(db, table_id)
            .unwrap_or(LuaType::Table)
    }

    fn transform_array(&mut self, array: Arc<LuaArrayType>, context: WideningContext) -> LuaType {
        let element_context = match context {
            WideningContext::TupleElement => WideningContext::TupleElement,
            _ => WideningContext::ArrayElement,
        };
        let base = self.transform(array.get_base().clone(), element_context);
        LuaType::Array(LuaArrayType::new(base, array.get_len().clone()).into())
    }

    fn transform_tuple(&mut self, tuple: Arc<LuaTupleType>) -> LuaType {
        let types = tuple
            .get_types()
            .iter()
            .cloned()
            .map(|ty| self.transform(ty, WideningContext::TupleElement))
            .collect();
        LuaType::Tuple(LuaTupleType::new(types, tuple.status).into())
    }

    fn transform_object(&mut self, object: Arc<LuaObjectType>) -> LuaType {
        let fields = object
            .get_fields()
            .iter()
            .map(|(key, ty)| {
                (
                    key.clone(),
                    self.transform(ty.clone(), WideningContext::ObjectProperty),
                )
            })
            .collect();
        let index_access = object
            .get_index_access()
            .iter()
            .map(|(key, value)| {
                (
                    self.transform(key.clone(), WideningContext::ObjectProperty),
                    self.transform(value.clone(), WideningContext::ObjectProperty),
                )
            })
            .collect();
        LuaType::Object(LuaObjectType::new_with_fields(fields, index_access).into())
    }

    fn transform_union(&mut self, union: Arc<LuaUnionType>, context: WideningContext) -> LuaType {
        let member_context = self.union_member_context(context);
        LuaType::Union(
            LuaUnionType::from_vec(
                union
                    .into_vec()
                    .into_iter()
                    .map(|ty| self.transform(ty, member_context))
                    .collect(),
            )
            .into(),
        )
    }

    fn transform_multi_line_union(
        &mut self,
        multi: Arc<crate::LuaMultiLineUnion>,
        context: WideningContext,
    ) -> LuaType {
        let member_context = self.union_member_context(context);
        LuaType::MultiLineUnion(
            crate::LuaMultiLineUnion::new(
                multi
                    .get_unions()
                    .iter()
                    .map(|(ty, description)| {
                        (
                            self.transform(ty.clone(), member_context),
                            description.clone(),
                        )
                    })
                    .collect(),
            )
            .into(),
        )
    }

    fn transform_intersection(
        &mut self,
        intersection: Arc<crate::LuaIntersectionType>,
        context: WideningContext,
    ) -> LuaType {
        let member_context = self.union_member_context(context);
        LuaType::Intersection(
            crate::LuaIntersectionType::new(
                intersection
                    .get_types()
                    .iter()
                    .cloned()
                    .map(|ty| self.transform(ty, member_context))
                    .collect(),
            )
            .into(),
        )
    }

    fn transform_variadic(&mut self, variadic: Arc<VariadicType>) -> LuaType {
        LuaType::Variadic(
            match variadic.deref() {
                VariadicType::Base(base) => VariadicType::Base(
                    self.transform(base.clone(), WideningContext::VariadicElement),
                ),
                VariadicType::Multi(types) => VariadicType::Multi(
                    types
                        .iter()
                        .cloned()
                        .map(|ty| self.transform(ty, WideningContext::VariadicElement))
                        .collect(),
                ),
            }
            .into(),
        )
    }

    fn transform_generic(&mut self, generic: Arc<LuaGenericType>) -> LuaType {
        LuaType::Generic(
            LuaGenericType::new(
                generic.get_base_type_id(),
                generic
                    .get_params()
                    .iter()
                    .cloned()
                    .map(|ty| self.transform(ty, WideningContext::Root))
                    .collect(),
            )
            .into(),
        )
    }

    fn transform_table_generic(&mut self, params: Arc<Vec<LuaType>>) -> LuaType {
        LuaType::TableGeneric(
            params
                .iter()
                .cloned()
                .map(|ty| self.transform(ty, WideningContext::Root))
                .collect::<Vec<_>>()
                .into(),
        )
    }

    fn transform_doc_function(&mut self, func: Arc<LuaFunctionType>) -> LuaType {
        LuaType::DocFunction(
            LuaFunctionType::new(
                func.get_async_state(),
                func.is_colon_define(),
                func.is_variadic(),
                func.get_params()
                    .iter()
                    .map(|(name, ty)| {
                        (
                            name.clone(),
                            ty.clone()
                                .map(|ty| self.transform(ty, WideningContext::Root)),
                        )
                    })
                    .collect(),
                self.transform(func.get_ret().clone(), WideningContext::Root),
            )
            .into(),
        )
    }

    fn transform_type_guard(&mut self, type_guard: Arc<LuaType>) -> LuaType {
        LuaType::TypeGuard(
            self.transform(type_guard.deref().clone(), WideningContext::Root)
                .into(),
        )
    }

    fn transform_conditional(&mut self, conditional: Arc<LuaConditionalType>) -> LuaType {
        LuaType::Conditional(
            LuaConditionalType::new(
                self.transform(
                    conditional.get_checked_type().clone(),
                    WideningContext::Root,
                ),
                self.transform(
                    conditional.get_extends_type().clone(),
                    WideningContext::Root,
                ),
                self.transform(conditional.get_true_type().clone(), WideningContext::Root),
                self.transform(conditional.get_false_type().clone(), WideningContext::Root),
                conditional.get_infer_params().to_vec(),
                conditional.has_new,
            )
            .into(),
        )
    }

    fn transform_mapped(&mut self, mapped: Arc<LuaMappedType>) -> LuaType {
        LuaType::Mapped(Arc::new(LuaMappedType::new(
            (
                mapped.param.0,
                GenericParam::new(
                    mapped.param.1.name.clone(),
                    mapped
                        .param
                        .1
                        .type_constraint
                        .clone()
                        .map(|ty| self.transform(ty, WideningContext::Root)),
                    mapped
                        .param
                        .1
                        .default_type
                        .clone()
                        .map(|ty| self.transform(ty, WideningContext::Root)),
                    mapped.param.1.attributes.clone(),
                ),
            ),
            self.transform(mapped.value.clone(), WideningContext::Root),
            mapped.is_readonly,
            mapped.is_optional,
        )))
    }

    fn transform_terminal(&self, ty: LuaType, context: WideningContext) -> LuaType {
        // Keep a top-level literal union intact. Widening `"a" | "b"` to `string`
        // would throw away a deliberate literal candidate during inference.
        if matches!(context, WideningContext::RootUnionMember) {
            return ty;
        }

        if matches!(context, WideningContext::Root)
            && matches!(
                self.root_primitive_behavior,
                RootPrimitiveBehavior::PreserveLiteral
            )
        {
            return ty;
        }

        widen_primitive_literal(ty)
    }

    fn union_member_context(&self, context: WideningContext) -> WideningContext {
        if matches!(context, WideningContext::Root) {
            WideningContext::RootUnionMember
        } else {
            WideningContext::UnionMember
        }
    }

    fn table_const_to_object(
        &mut self,
        db: &DbIndex,
        table_id: InFiled<TextRange>,
    ) -> Option<LuaType> {
        if !self.guard.enter_table(&table_id) {
            return Some(LuaType::Table);
        }

        let owner = LuaMemberOwner::Element(table_id.clone());
        let members = match db.get_member_index().get_members(&owner) {
            Some(members) => members,
            None => {
                self.guard.leave_table(&table_id);
                return None;
            }
        };
        let mut fields = HashMap::with_capacity(members.len());
        let mut index_access = Vec::with_capacity(members.len());

        for member in members {
            let value = db
                .get_type_index()
                .get_type_cache(&member.get_id().into())
                .map(|cache| cache.as_type().clone())
                .unwrap_or(LuaType::Unknown);
            let value = self.transform(value, WideningContext::ObjectProperty);

            match member.get_key() {
                LuaMemberKey::Name(_) | LuaMemberKey::Integer(_) => {
                    let member_key = member.get_key().clone();
                    fields
                        .entry(member_key)
                        .and_modify(|prev| {
                            *prev = TypeOps::Union.apply(db, prev, &value);
                        })
                        .or_insert(value);
                }
                LuaMemberKey::ExprType(key) => {
                    index_access.push((
                        self.transform(key.clone(), WideningContext::ObjectProperty),
                        value,
                    ));
                }
                LuaMemberKey::None => {}
            }
        }

        self.guard.leave_table(&table_id);

        Some(LuaType::Object(
            LuaObjectType::new_with_fields(fields, index_access).into(),
        ))
    }
}

pub(in crate::semantic::generic) fn regularize_tpl_candidate_type(
    db: &DbIndex,
    ty: LuaType,
) -> LuaType {
    WideningTransformer::for_candidate_regularization(db).transform(ty, WideningContext::Root)
}

pub(in crate::semantic::generic) fn widen_tpl_candidate_type(db: &DbIndex, ty: LuaType) -> LuaType {
    WideningTransformer::for_candidate_widening(db).transform(ty, WideningContext::Root)
}

fn widen_primitive_literal(ty: LuaType) -> LuaType {
    match ty {
        LuaType::FloatConst(_) => LuaType::Number,
        LuaType::DocIntegerConst(_) | LuaType::IntegerConst(_) => LuaType::Integer,
        LuaType::DocStringConst(_) | LuaType::StringConst(_) => LuaType::String,
        LuaType::DocBooleanConst(_) | LuaType::BooleanConst(_) => LuaType::Boolean,
        ty => ty,
    }
}
