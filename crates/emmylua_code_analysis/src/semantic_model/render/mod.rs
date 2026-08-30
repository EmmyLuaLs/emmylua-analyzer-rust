//! # render — unified human-readable `LuaType` rendering
//!
//! A lightweight type humanizer built on the new `SemanticModel` (salsa).
//! It aims to replace the ad-hoc `humanize` implementations maintained in
//! `test_lib` / `emmylua_ls` by providing a unified rendering entry point
//! with depth limits and cycle detection.

use std::collections::HashSet;
use std::fmt::{self, Write};
use std::sync::Arc;

use crate::{
    AsyncState, GenericTplId, LuaFunctionType, LuaMemberKey, LuaType, LuaTypeDeclId, LuaUnionType,
    VariadicType,
};

use super::{SemanticModel, infer};

// ─── RenderLevel ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLevel {
    Documentation,
    Detailed,
    Simple,
    Normal,
    Brief,
    Minimal,
}

impl RenderLevel {
    pub fn next_level(self) -> RenderLevel {
        match self {
            RenderLevel::Documentation => RenderLevel::Simple,
            RenderLevel::Detailed => RenderLevel::Simple,
            RenderLevel::Simple => RenderLevel::Normal,
            RenderLevel::Normal => RenderLevel::Brief,
            RenderLevel::Brief => RenderLevel::Minimal,
            RenderLevel::Minimal => RenderLevel::Minimal,
        }
    }

    fn max_items(self) -> usize {
        match self {
            RenderLevel::Documentation => 500,
            RenderLevel::Detailed => 10,
            RenderLevel::Simple => 8,
            RenderLevel::Normal => 4,
            RenderLevel::Brief => 2,
            RenderLevel::Minimal => 2,
        }
    }

    fn max_union_items(self) -> usize {
        match self {
            RenderLevel::Documentation => 500,
            RenderLevel::Detailed => 8,
            RenderLevel::Simple => 6,
            RenderLevel::Normal => 4,
            RenderLevel::Brief => 2,
            RenderLevel::Minimal => 2,
        }
    }
}

// ─── TypeHumanizer ──────────────────────────────────────────────────────────

const DEFAULT_MAX_DEPTH: u8 = 12;

/// A writer-based humanizer that avoids intermediate `String` allocation and has depth limits plus recursive alias cycle detection.
pub struct TypeHumanizer<'a> {
    model: &'a SemanticModel<'a>,
    level: RenderLevel,
    depth: u8,
    max_depth: u8,
    /// When rendering generic default arguments, whether to keep expanding defaults of referenced types (avoids recursive expansion).
    expand_defaults: bool,
}

impl<'a> TypeHumanizer<'a> {
    pub fn new(model: &'a SemanticModel<'a>, level: RenderLevel) -> Self {
        Self {
            model,
            level,
            depth: 0,
            max_depth: DEFAULT_MAX_DEPTH,
            expand_defaults: true,
        }
    }

    fn guard(&mut self) -> Option<DepthGuardToken> {
        if self.depth >= self.max_depth {
            return None;
        }
        self.depth += 1;
        Some(DepthGuardToken)
    }

    fn leave_guard(&mut self, _token: DepthGuardToken) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn child_level(&self) -> RenderLevel {
        self.level.next_level()
    }

    pub fn write_type<W: Write>(&mut self, ty: &LuaType, w: &mut W) -> fmt::Result {
        let token = match self.guard() {
            Some(t) => t,
            None => return w.write_str("..."),
        };
        let result = self.write_type_inner(ty, w);
        self.leave_guard(token);
        result
    }

    fn write_type_inner<W: Write>(&mut self, ty: &LuaType, w: &mut W) -> fmt::Result {
        match ty {
            LuaType::Any => w.write_str("any"),
            LuaType::Nil => w.write_str("nil"),
            LuaType::Boolean => w.write_str("boolean"),
            LuaType::Number => w.write_str("number"),
            LuaType::String => w.write_str("string"),
            LuaType::Integer => w.write_str("integer"),
            LuaType::Table => w.write_str("table"),
            LuaType::Function => w.write_str("function"),
            LuaType::Thread => w.write_str("thread"),
            LuaType::Userdata => w.write_str("userdata"),
            LuaType::Io => w.write_str("io"),
            LuaType::Global => w.write_str("global"),
            LuaType::SelfInfer => w.write_str("self"),
            LuaType::Unknown => w.write_str("unknown"),
            LuaType::Never => w.write_str("never"),
            LuaType::BooleanConst(b) => write!(w, "{}", b),
            LuaType::DocBooleanConst(b) => write!(w, "{}", b),
            LuaType::IntegerConst(i) => write!(w, "{}", i),
            LuaType::DocIntegerConst(i) => write!(w, "{}", i),
            LuaType::FloatConst(f) => {
                let s = f.to_string();
                if s.contains('.') {
                    w.write_str(&s)
                } else {
                    write!(w, "{}.0", s)
                }
            }
            LuaType::StringConst(s) => write_escaped_string(s, w),
            LuaType::DocStringConst(s) => write_escaped_string(s, w),
            LuaType::TableConst(_) => {
                // Anonymous table literals expand to named objects in Detailed/Normal rendering rather than a generic `table`;
                // Runtime and declared members are also expanded uniformly through `member_infos`.
                let infos = self.model.member_infos(ty);
                if infos.is_empty() {
                    w.write_str("table")
                } else {
                    let fields: Vec<(LuaMemberKey, LuaType)> =
                        infos.into_iter().map(|info| (info.key, info.typ)).collect();
                    if self.level == RenderLevel::Detailed {
                        self.write_table_literal_detailed(&fields, w)
                    } else {
                        self.write_object(&fields, &[], w)
                    }
                }
            }
            LuaType::Ref(id) => self.write_ref(id, w),
            LuaType::Def(id) => self.write_ref(id, w),
            LuaType::Array(array) => {
                let saved = self.level;
                self.level = self.child_level();
                self.write_type(array.get_base(), w)?;
                self.level = saved;
                w.write_str("[]")
            }
            LuaType::Tuple(tuple) => {
                let types = tuple.get_types();
                let num = self.level.max_items();
                w.write_char('(')?;
                let saved = self.level;
                self.level = self.child_level();
                for (i, ty) in types.iter().take(num).enumerate() {
                    if i > 0 {
                        w.write_char(',')?;
                    }
                    self.write_type(ty, w)?;
                }
                self.level = saved;
                if types.len() > num {
                    w.write_str("...")?;
                }
                w.write_char(')')
            }
            LuaType::Union(union) => self.write_union(union, w),
            LuaType::Intersection(intersection) => {
                let types = intersection.get_types();
                let num = self.level.max_items();
                w.write_char('(')?;
                let saved = self.level;
                self.level = self.child_level();
                for (i, ty) in types.iter().take(num).enumerate() {
                    if i > 0 {
                        w.write_str(" & ")?;
                    }
                    self.write_type(ty, w)?;
                }
                self.level = saved;
                if types.len() > num {
                    w.write_str(", ...")?;
                }
                w.write_char(')')
            }
            LuaType::Generic(generic) => {
                self.write_generic(&generic.get_base_type_id(), generic.get_params(), w)
            }
            LuaType::TableGeneric(params) => {
                if self.level == RenderLevel::Minimal {
                    return w.write_str("table<...>");
                }
                w.write_str("table<")?;
                let saved = self.level;
                self.level = self.child_level();
                for (i, ty) in params.iter().enumerate() {
                    if i > 0 {
                        w.write_char(',')?;
                    }
                    self.write_type(ty, w)?;
                }
                self.level = saved;
                w.write_char('>')
            }
            LuaType::DocFunction(fun) => self.write_function(fun, w),
            LuaType::Object(object) => {
                let fields: Vec<(LuaMemberKey, LuaType)> = object
                    .get_fields()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                self.write_object(&fields, object.get_index_access(), w)
            }
            LuaType::TplRef(tpl) => w.write_str(tpl.get_name()),
            LuaType::StrTplRef(str_tpl) => {
                if str_tpl.get_prefix().is_empty() {
                    w.write_str(str_tpl.get_name())
                } else {
                    write!(w, "{}`{}`", str_tpl.get_prefix(), str_tpl.get_name())
                }
            }
            LuaType::Variadic(variadic) => self.write_variadic(variadic, w),
            LuaType::Instance(instance) => self.write_type(instance.get_base(), w),
            LuaType::TypeGuard(inner) => {
                w.write_str("TypeGuard<")?;
                let saved = self.level;
                self.level = self.child_level();
                self.write_type(inner, w)?;
                self.level = saved;
                w.write_char('>')
            }
            LuaType::MultiLineUnion(multi_union) => {
                let members = multi_union.get_unions();
                let num = self.level.max_items();
                w.write_char('(')?;
                let saved = self.level;
                self.level = self.child_level();
                for (i, (ty, _)) in members.iter().take(num).enumerate() {
                    if i > 0 {
                        w.write_char('|')?;
                    }
                    self.write_type(ty, w)?;
                }
                self.level = saved;
                if members.len() > num {
                    w.write_str("...")?;
                }
                w.write_char(')')
            }
            LuaType::Conditional(conditional) => {
                let saved = self.level;
                self.level = self.child_level();
                self.write_type(conditional.get_checked_type(), w)?;
                w.write_str(" extends ")?;
                self.write_type(conditional.get_extends_type(), w)?;
                w.write_str(" ? ")?;
                self.write_type(conditional.get_true_type(), w)?;
                w.write_str(" : ")?;
                self.write_type(conditional.get_false_type(), w)?;
                self.level = saved;
                Ok(())
            }
            LuaType::Mapped(mapped) => self.write_mapped_type(mapped, w),
            LuaType::Call(call) => {
                if let Some(expanded) = super::type_eval::expand_index_call(self.model, call) {
                    self.write_type(&expanded, w)?;
                } else {
                    w.write_str("call<")?;
                    let saved = self.level;
                    self.level = self.child_level();
                    for (i, ty) in call.get_operands().iter().enumerate() {
                        if i > 0 {
                            w.write_char(',')?;
                        }
                        self.write_type(ty, w)?;
                    }
                    self.level = saved;
                    w.write_char('>')?;
                }
                Ok(())
            }
            LuaType::Namespace(ns) => write!(w, "{{ {} }}", ns),
            LuaType::Language(s) => w.write_str(s),
            LuaType::Signature(_) => w.write_str("fun(...) -> ..."),
            LuaType::ModuleRef(_) => w.write_str("module"),
        }
    }

    /// Resolves default generic arguments of a type definition, supporting forward/backward references and iterating to a fixpoint.
    fn resolve_generic_defaults(&self, def: &crate::TypeDef, provided: &[LuaType]) -> Vec<LuaType> {
        let n = def.generic_params.len();
        let mut resolved: Vec<Option<LuaType>> = vec![None; n];
        for (i, ty) in provided.iter().enumerate() {
            if i < n {
                resolved[i] = Some(ty.clone());
            }
        }
        for _ in 0..n {
            let mut changed = false;
            for (i, param) in def.generic_params.iter().enumerate() {
                if i < provided.len() {
                    continue;
                }
                let Some(syntax) = param.default else {
                    continue;
                };
                let mut ty = self
                    .model
                    .doc_type_lua_in(def.file_id, syntax, &def.generic_params);
                let mut bindings = infer::unify::TplBindings::new();
                for (j, value) in resolved.iter().enumerate() {
                    if let Some(value) = value {
                        bindings.insert(GenericTplId::Type(j as u32), value.clone());
                    }
                }
                ty = infer::unify::substitute(&ty, &bindings);
                if resolved[i].as_ref() != Some(&ty) {
                    resolved[i] = Some(ty);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        resolved
            .into_iter()
            .map(|ty| ty.unwrap_or(LuaType::Unknown))
            .collect()
    }

    fn write_ref<W: Write>(&mut self, id: &LuaTypeDeclId, w: &mut W) -> fmt::Result {
        if let Some(def) = self.model.type_def_of(id) {
            w.write_str(def.full_name.as_str())?;
            // Bare generic types: fill in all default arguments for display (the actual type remains a `Ref`).
            let mut resolved_params: Option<Vec<LuaType>> = None;
            if self.expand_defaults
                && !def.generic_params.is_empty()
                && def
                    .generic_params
                    .iter()
                    .all(|param| param.default.is_some())
            {
                w.write_char('<')?;
                let saved = self.level;
                self.level = self.child_level();
                let old_expand = self.expand_defaults;
                self.expand_defaults = false;
                let resolved = self.resolve_generic_defaults(&def, &[]);
                for (i, ty) in resolved.iter().enumerate() {
                    if i > 0 {
                        w.write_char(',')?;
                    }
                    self.write_type(ty, w)?;
                }
                self.expand_defaults = old_expand;
                self.level = saved;
                w.write_char('>')?;
                resolved_params = Some(resolved);
            }
            // Detailed mode: aliases render as `Alias<...> = target type`.
            if self.level == RenderLevel::Detailed && def.kind == crate::TypeDefKind::Alias {
                if let Some(target) = self.model.alias_target(&def) {
                    let mut bindings = std::collections::HashMap::new();
                    if let Some(resolved) = &resolved_params {
                        for (index, ty) in resolved.iter().enumerate() {
                            bindings.insert(GenericTplId::Type(index as u32), ty.clone());
                        }
                    } else {
                        let resolved = self.resolve_generic_defaults(&def, &[]);
                        for (index, ty) in resolved.iter().enumerate() {
                            bindings.insert(GenericTplId::Type(index as u32), ty.clone());
                        }
                    }
                    let target = infer::unify::substitute(&target, &bindings);
                    w.write_str(" = ")?;
                    let saved = self.level;
                    self.level = self.child_level();
                    self.write_type(&target, w)?;
                    self.level = saved;
                }
            }
            Ok(())
        } else {
            w.write_str(id.get_simple_name())
        }
    }

    fn write_generic<W: Write>(
        &mut self,
        base_id: &LuaTypeDeclId,
        params: &[LuaType],
        w: &mut W,
    ) -> fmt::Result {
        let def = self.model.type_def_of(base_id);
        if let Some(def) = &def {
            w.write_str(def.full_name.as_str())?;
        } else {
            w.write_str(base_id.get_simple_name())?;
        }
        w.write_char('<')?;
        let saved = self.level;
        self.level = self.child_level();
        let resolved = if let Some(def) = &def {
            self.resolve_generic_defaults(def, params)
        } else {
            params.to_vec()
        };
        for (i, ty) in resolved.iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            self.write_type(ty, w)?;
        }
        self.level = saved;
        w.write_char('>')?;
        // Detailed mode: generic aliases render as `Alias<...> = target type`.
        if self.level == RenderLevel::Detailed
            && let Some(def) = &def
            && def.kind == crate::TypeDefKind::Alias
            && let Some(target) = self.model.alias_target(def)
        {
            let mut bindings = std::collections::HashMap::new();
            let mut name_bindings: std::collections::HashMap<String, LuaType> =
                std::collections::HashMap::new();
            for (index, ty) in resolved.iter().enumerate() {
                bindings.insert(GenericTplId::Type(index as u32), ty.clone());
                if let Some(param) = def.generic_params.get(index) {
                    name_bindings.insert(param.name.to_string(), ty.clone());
                }
            }
            let target = self.substitute_named_refs(&target, &name_bindings);
            let target = infer::unify::substitute(&target, &bindings);
            let target = super::type_eval::expand_alias_generic(self.model, &target);
            let target = super::type_eval::eval_conditionals(self.model, &target);
            w.write_str(" = ")?;
            let saved = self.level;
            self.level = self.child_level();
            self.write_type(&target, w)?;
            self.level = saved;
        }
        Ok(())
    }

    fn write_union<W: Write>(&mut self, union: &LuaUnionType, w: &mut W) -> fmt::Result {
        let types = union.into_vec();
        let num = self.level.max_union_items();

        let mut seen = HashSet::new();
        let mut unique: Vec<LuaType> = Vec::new();
        let mut has_nil = false;
        let mut has_function = false;

        for ty in types.iter() {
            if ty.is_nil() {
                has_nil = true;
                continue;
            }
            if ty.is_function() {
                has_function = true;
            }
            let mut key = String::new();
            let saved = self.level;
            self.level = self.child_level();
            let _ = self.write_type(ty, &mut key);
            self.level = saved;
            if seen.insert(key) {
                unique.push(ty.clone());
            }
        }

        unique.sort_by_key(|ty| self.union_type_rank(ty));

        let total = unique.len();
        let show_dots = total > num;
        let needs_parens = total > 1 || (total == 1 && has_function && has_nil);

        if needs_parens {
            w.write_char('(')?;
        }

        for (i, ty) in unique.iter().take(num).enumerate() {
            if i > 0 {
                w.write_char('|')?;
            }
            self.write_type(ty, w)?;
        }

        if show_dots {
            w.write_str("...")?;
        }

        if needs_parens {
            w.write_char(')')?;
        }

        if has_nil {
            w.write_char('?')?;
        }

        Ok(())
    }

    fn union_type_rank(&self, ty: &LuaType) -> u8 {
        match ty {
            LuaType::Ref(_)
            | LuaType::Def(_)
            | LuaType::Generic(_)
            | LuaType::TplRef(_)
            | LuaType::StrTplRef(_)
            | LuaType::Object(_)
            | LuaType::Array(_)
            | LuaType::TableGeneric(_)
            | LuaType::DocFunction(_)
            | LuaType::Call(_) => 0,
            LuaType::BooleanConst(true) | LuaType::DocBooleanConst(true) => 1,
            LuaType::BooleanConst(false) | LuaType::DocBooleanConst(false) => 2,
            _ => 3,
        }
    }

    fn write_variadic<W: Write>(&mut self, variadic: &VariadicType, w: &mut W) -> fmt::Result {
        match variadic {
            VariadicType::Base(base) => {
                self.write_type(base, w)?;
                w.write_str("...")
            }
            VariadicType::Multi(types) => {
                if self.level == RenderLevel::Minimal {
                    return w.write_str("multi<...>");
                }
                w.write_char('(')?;
                let saved = self.level;
                self.level = self.child_level();
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        w.write_char(',')?;
                    }
                    self.write_type(ty, w)?;
                }
                self.level = saved;
                w.write_char(')')
            }
        }
    }

    fn write_function<W: Write>(&mut self, fun: &LuaFunctionType, w: &mut W) -> fmt::Result {
        if self.level == RenderLevel::Minimal {
            return w.write_str("fun(...) -> ...");
        }

        match fun.get_async_state() {
            AsyncState::None => w.write_str("fun")?,
            AsyncState::Async => w.write_str("async fun")?,
            AsyncState::Sync => w.write_str("sync fun")?,
        }

        w.write_char('(')?;
        let saved = self.level;
        self.level = self.child_level();
        for (i, (name, ty)) in fun.get_params().iter().enumerate() {
            if i > 0 {
                w.write_str(", ")?;
            }
            w.write_str(name)?;
            if let Some(ty) = ty {
                w.write_str(": ")?;
                self.write_type(ty, w)?;
            }
        }
        self.level = saved;
        w.write_char(')')?;

        let ret = fun.get_ret();
        if ret.is_nil() || matches!(ret, LuaType::Unknown) {
            return Ok(());
        }

        w.write_str(" -> ")?;
        let saved = self.level;
        self.level = self.child_level();
        self.write_type(ret, w)?;
        self.level = saved;
        Ok(())
    }

    fn write_object<W: Write>(
        &mut self,
        fields: &[(LuaMemberKey, LuaType)],
        index_access: &[(LuaType, LuaType)],
        w: &mut W,
    ) -> fmt::Result {
        if self.level == RenderLevel::Minimal {
            return w.write_str("{...}");
        }

        let mut field_vec: Vec<&(LuaMemberKey, LuaType)> = fields.iter().collect();
        field_vec.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)));

        w.write_str("{ ")?;
        let saved = self.level;
        self.level = self.child_level();

        for (i, (key, ty)) in field_vec.iter().take(self.level.max_items()).enumerate() {
            if i > 0 {
                w.write_str(", ")?;
            }
            self.write_member_key(key, w)?;
            w.write_str(": ")?;
            self.write_type(ty, w)?;
        }

        if fields.len() > self.level.max_items() {
            w.write_str(", ...")?;
        }

        if !index_access.is_empty() {
            if !field_vec.is_empty() {
                w.write_str(", ")?;
            }
            for (i, (key, value)) in index_access.iter().enumerate() {
                if i > 0 {
                    w.write_char(',')?;
                }
                w.write_char('[')?;
                self.write_type(key, w)?;
                w.write_str("]: ")?;
                self.write_type(value, w)?;
            }
        }

        self.level = saved;
        w.write_str(" }")
    }

    fn substitute_named_refs(
        &self,
        ty: &LuaType,
        map: &std::collections::HashMap<String, LuaType>,
    ) -> LuaType {
        use LuaType::*;
        match ty {
            Ref(id) | Def(id) => map
                .get(id.get_name())
                .cloned()
                .unwrap_or_else(|| ty.clone()),
            Array(array) => Array(Arc::new(crate::LuaArrayType::from_base_type(
                self.substitute_named_refs(array.get_base(), map),
            ))),
            Tuple(tuple) => Tuple(Arc::new(crate::LuaTupleType::new(
                tuple
                    .get_types()
                    .iter()
                    .map(|t| self.substitute_named_refs(t, map))
                    .collect(),
                tuple.status,
            ))),
            Union(union) => Union(Arc::new(LuaUnionType::from_vec(
                union
                    .into_vec()
                    .iter()
                    .map(|t| self.substitute_named_refs(t, map))
                    .collect(),
            ))),
            Object(object) => Object(Arc::new(crate::LuaObjectType::new_with_fields(
                object
                    .get_fields()
                    .iter()
                    .map(|(k, v)| (k.clone(), self.substitute_named_refs(v, map)))
                    .collect(),
                object
                    .get_index_access()
                    .iter()
                    .map(|(k, v)| {
                        (
                            self.substitute_named_refs(k, map),
                            self.substitute_named_refs(v, map),
                        )
                    })
                    .collect(),
            ))),
            Variadic(variadic) => Variadic(Arc::new(match variadic.as_ref() {
                VariadicType::Base(base) => {
                    VariadicType::Base(self.substitute_named_refs(base, map))
                }
                VariadicType::Multi(types) => VariadicType::Multi(
                    types
                        .iter()
                        .map(|t| self.substitute_named_refs(t, map))
                        .collect(),
                ),
            })),
            Call(call) => Call(Arc::new(crate::LuaAliasCallType::new(
                call.get_call_kind(),
                call.get_operands()
                    .iter()
                    .map(|t| self.substitute_named_refs(t, map))
                    .collect(),
            ))),
            Mapped(mapped) => {
                let constraint = mapped
                    .param
                    .1
                    .constraint
                    .as_ref()
                    .map(|t| self.substitute_named_refs(t, map));
                let default = mapped
                    .param
                    .1
                    .default
                    .as_ref()
                    .map(|t| self.substitute_named_refs(t, map));
                Mapped(Arc::new(crate::LuaMappedType::new(
                    (
                        mapped.param.0,
                        crate::GenericParam::new(
                            mapped.param.1.name.clone(),
                            constraint,
                            default,
                            mapped.param.1.is_const,
                            mapped.param.1.attributes.clone(),
                        ),
                    ),
                    self.substitute_named_refs(&mapped.value, map),
                    mapped.is_readonly,
                    mapped.is_optional,
                )))
            }
            Conditional(conditional) => Conditional(Arc::new(crate::LuaConditionalType::new(
                self.substitute_named_refs(conditional.get_checked_type(), map),
                self.substitute_named_refs(conditional.get_extends_type(), map),
                self.substitute_named_refs(conditional.get_true_type(), map),
                self.substitute_named_refs(conditional.get_false_type(), map),
                conditional.get_infer_params().to_vec(),
                conditional.has_new,
            ))),
            _ => ty.clone(),
        }
    }

    /// Mapped types: prefer semantic expansion into an object; keep the mapped structure when expansion fails.
    fn write_mapped_type<W: Write>(
        &mut self,
        mapped: &crate::LuaMappedType,
        w: &mut W,
    ) -> fmt::Result {
        if let Some(expanded) = super::type_eval::expand_mapped(self.model, mapped) {
            return self.write_type(&expanded, w);
        }
        w.write_str("{ [")?;
        w.write_str(mapped.param.1.name.as_str())?;
        w.write_str(" in ")?;
        let saved = self.level;
        self.level = self.child_level();
        if let Some(constraint) = &mapped.param.1.constraint {
            self.write_type(constraint, w)?;
        } else {
            w.write_str("unknown")?;
        }
        self.level = saved;
        w.write_str("]: ")?;
        self.write_type(&mapped.value, w)?;
        w.write_str(" }")
    }

    /// Detailed mode keeps the legacy per-line object format for anonymous table literals.
    fn write_table_literal_detailed<W: Write>(
        &mut self,
        fields: &[(LuaMemberKey, LuaType)],
        w: &mut W,
    ) -> fmt::Result {
        if self.level == RenderLevel::Minimal {
            return w.write_str("{...}");
        }
        w.write_str("{\n")?;
        let saved = self.level;
        self.level = self.child_level();
        for (key, ty) in fields {
            w.write_str("    ")?;
            self.write_member_key(key, w)?;
            w.write_str(": ")?;
            self.write_type(ty, w)?;
            // The legacy detailed format keeps a trailing comma.
            w.write_char(',')?;
            w.write_char('\n')?;
        }
        self.level = saved;
        w.write_str("}")
    }

    fn write_member_key<W: Write>(&mut self, key: &LuaMemberKey, w: &mut W) -> fmt::Result {
        match key {
            LuaMemberKey::Name(name) => w.write_str(name),
            LuaMemberKey::Integer(i) => write!(w, "[{}]", i),
            LuaMemberKey::TypeKey(ty) => {
                w.write_char('[')?;
                self.write_type(ty, w)?;
                w.write_char(']')
            }
            LuaMemberKey::None => Ok(()),
        }
    }
}

// ─── Free helpers ───────────────────────────────────────────────────────────

struct DepthGuardToken;

fn write_escaped_string<W: Write>(s: &str, w: &mut W) -> fmt::Result {
    w.write_char('"')?;
    for ch in s.chars() {
        match ch {
            '\\' => w.write_str("\\\\")?,
            '"' => w.write_str("\\\"")?,
            '\n' => w.write_str("\\n")?,
            '\r' => w.write_str("\\r")?,
            '\t' => w.write_str("\\t")?,
            '\u{1b}' => w.write_str("\\27")?,
            ch if ch.is_control() => {
                let code = ch as u32;
                if code <= 0xFF {
                    write!(w, "\\x{code:02X}")?;
                } else {
                    write!(w, "\\u{{{code:X}}}")?;
                }
            }
            _ => w.write_char(ch)?,
        }
    }
    w.write_char('"')
}

// ─── Public entry points ────────────────────────────────────────────────────

pub fn humanize_type(model: &SemanticModel, ty: &LuaType) -> String {
    humanize_type_with_level(model, ty, RenderLevel::Simple)
}

pub fn humanize_type_detailed(model: &SemanticModel, ty: &LuaType) -> String {
    humanize_type_with_level(model, ty, RenderLevel::Detailed)
}

pub fn humanize_type_with_level(model: &SemanticModel, ty: &LuaType, level: RenderLevel) -> String {
    let mut humanizer = TypeHumanizer::new(model, level);
    let mut buf = String::new();
    let _ = humanizer.write_type(ty, &mut buf);
    buf
}
