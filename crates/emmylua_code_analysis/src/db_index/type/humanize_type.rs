use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write};

use itertools::Itertools;

use crate::{
    AsyncState, DbIndex, LuaAliasCallType, LuaConditionalType, LuaFunctionType, LuaGenericType,
    LuaInstanceType, LuaIntersectionType, LuaMemberKey, LuaMemberOwner, LuaObjectType,
    LuaSignatureId, LuaStringTplType, LuaTupleType, LuaType, LuaTypeDeclId, LuaUnionType, TypeOps,
    TypeSubstitutor, VariadicType,
};

use super::{LuaAliasCallKind, LuaMultiLineUnion};

// ─── RenderLevel ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLevel {
    Documentation,
    // do not set more than 255
    CustomDetailed(u8),
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
            RenderLevel::CustomDetailed(_) => RenderLevel::Simple,
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
            RenderLevel::CustomDetailed(n) => n as usize,
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
            RenderLevel::CustomDetailed(n) => n as usize,
            RenderLevel::Detailed => 8,
            RenderLevel::Simple => 6,
            RenderLevel::Normal => 4,
            RenderLevel::Brief => 2,
            RenderLevel::Minimal => 2,
        }
    }

    fn max_display_count(self) -> Option<usize> {
        match self {
            RenderLevel::Documentation => Some(500),
            RenderLevel::CustomDetailed(n) => Some(n as usize),
            RenderLevel::Detailed => Some(12),
            _ => None,
        }
    }
}

// ─── TypeHumanizer ──────────────────────────────────────────────────────────

const DEFAULT_MAX_DEPTH: u8 = 12;

/// Core writer-based type humanizer. Avoids intermediate `String` allocations
/// and prevents infinite recursion through depth tracking and cycle detection.
pub struct TypeHumanizer<'a> {
    db: &'a DbIndex,
    level: RenderLevel,
    depth: u8,
    max_depth: u8,
    /// Tracks visited `LuaTypeDeclId`s to break cycles from recursive aliases / refs.
    visited: HashSet<LuaTypeDeclId>,
}

impl<'a> TypeHumanizer<'a> {
    pub fn new(db: &'a DbIndex, level: RenderLevel) -> Self {
        Self {
            db,
            level,
            depth: 0,
            max_depth: DEFAULT_MAX_DEPTH,
            visited: HashSet::new(),
        }
    }

    pub fn with_max_depth(mut self, max_depth: u8) -> Self {
        self.max_depth = max_depth;
        self
    }

    // ─── depth guard ────────────────────────────────────────────────

    /// Try to enter a deeper recursion level. Returns `None` if depth limit
    /// is reached; the caller should write `"..."` and return. Otherwise
    /// returns a token that must be passed to `leave_guard` when done.
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

    /// The child level (one step less detailed) for nested types.
    fn child_level(&self) -> RenderLevel {
        self.level.next_level()
    }

    // ─── public entry point ─────────────────────────────────────────

    /// Write the humanized representation of `ty` into `w`.
    pub fn write_type<W: Write>(&mut self, ty: &LuaType, w: &mut W) -> fmt::Result {
        let token = match self.guard() {
            Some(t) => t,
            None => return w.write_str("..."),
        };

        let result = self.write_type_inner(ty, w);

        self.leave_guard(token);
        result
    }

    // ─── main dispatcher ────────────────────────────────────────────

    fn write_type_inner<W: Write>(&mut self, ty: &LuaType, w: &mut W) -> fmt::Result {
        match ty {
            LuaType::Any => w.write_str("any"),
            LuaType::Nil => w.write_str("nil"),
            LuaType::Boolean => w.write_str("boolean"),
            LuaType::Number => w.write_str("number"),
            LuaType::String => w.write_str("string"),
            LuaType::Table => w.write_str("table"),
            LuaType::Function => w.write_str("function"),
            LuaType::Thread => w.write_str("thread"),
            LuaType::Userdata => w.write_str("userdata"),
            LuaType::IntegerConst(i) => write!(w, "{}", i),
            LuaType::FloatConst(f) => {
                let s = f.to_string();
                if !s.contains('.') {
                    write!(w, "{}.0", s)
                } else {
                    w.write_str(&s)
                }
            }
            LuaType::TableConst(v) => {
                let member_owner = LuaMemberOwner::Element(v.clone());
                self.write_table_const_type(member_owner, w)
            }
            LuaType::Global => w.write_str("global"),
            LuaType::Def(id) => self.write_def_type(id, w),
            LuaType::Union(union) => self.write_union_type(union, w),
            LuaType::Tuple(tuple) => self.write_tuple_type(tuple, w),
            LuaType::Unknown => w.write_str("unknown"),
            LuaType::Integer => w.write_str("integer"),
            LuaType::Io => w.write_str("io"),
            LuaType::SelfInfer => w.write_str("self"),
            LuaType::BooleanConst(b) => write!(w, "{}", b),
            LuaType::StringConst(s) => {
                w.write_char('"')?;
                write_hover_escape_string(s, w)?;
                w.write_char('"')
            }
            LuaType::DocStringConst(s) => {
                w.write_char('"')?;
                write_hover_escape_string(s, w)?;
                w.write_char('"')
            }
            LuaType::DocIntegerConst(i) => write!(w, "{}", i),
            LuaType::DocBooleanConst(b) => write!(w, "{}", b),
            LuaType::Ref(id) => self.write_ref_type(id, w),
            LuaType::Array(arr_inner) => self.write_array_type(arr_inner.get_base(), w),
            LuaType::Call(alias_call) => self.write_call_type(alias_call, w),
            LuaType::DocFunction(lua_func) => self.write_doc_function_type(lua_func, w),
            LuaType::Object(object) => self.write_object_type(object, w),
            LuaType::Intersection(inter) => self.write_intersect_type(inter, w),
            LuaType::Generic(generic) => self.write_generic_type(generic, w),
            LuaType::TableGeneric(table_generic_params) => {
                self.write_table_generic_type(table_generic_params, w)
            }
            LuaType::TplRef(tpl) => w.write_str(tpl.get_name()),
            LuaType::StrTplRef(str_tpl) => self.write_str_tpl_ref_type(str_tpl, w),
            LuaType::Variadic(multi) => self.write_variadic_type(multi, w),
            LuaType::Instance(ins) => self.write_instance_type(ins, w),
            LuaType::Signature(signature_id) => self.write_signature_type(signature_id, w),
            LuaType::Namespace(ns) => write!(w, "{{ {} }}", ns),
            LuaType::MultiLineUnion(multi_union) => {
                self.write_multi_line_union_type(multi_union, w)
            }
            LuaType::TypeGuard(inner) => {
                w.write_str("TypeGuard<")?;
                let saved = self.level;
                self.level = self.child_level();
                self.write_type(inner, w)?;
                self.level = saved;
                w.write_char('>')
            }
            LuaType::ConstTplRef(const_tpl) => w.write_str(const_tpl.get_name()),
            LuaType::Language(s) => w.write_str(s),
            LuaType::Conditional(c) => self.write_conditional_type(c, w),
            LuaType::ConditionalInfer(s) => w.write_str(s),
            LuaType::Never => w.write_str("never"),
            LuaType::ModuleRef(file_id) => self.write_module_ref(*file_id, w),
            _ => w.write_str("unknown"),
        }
    }

    // ─── Ref ────────────────────────────────────────────────────────

    fn write_ref_type<W: Write>(&mut self, id: &LuaTypeDeclId, w: &mut W) -> fmt::Result {
        if let Some(type_decl) = self.db.get_type_index().get_type_decl(id) {
            let name = type_decl.get_full_name().to_string();
            match self.write_simple_type(id, &name, w) {
                Ok(true) => Ok(()),
                Ok(false) => w.write_str(&name),
                Err(e) => Err(e),
            }
        } else {
            w.write_str(id.get_name())
        }
    }

    // ─── Def ────────────────────────────────────────────────────────

    fn write_def_type<W: Write>(&mut self, id: &LuaTypeDeclId, w: &mut W) -> fmt::Result {
        let type_decl = match self.db.get_type_index().get_type_decl(id) {
            Some(type_decl) => type_decl,
            None => return w.write_str(id.get_name()),
        };

        let full_name = type_decl.get_full_name().to_string();
        let generic = match self.db.get_type_index().get_generic_params(id) {
            Some(generic) => generic,
            None => {
                return match self.write_simple_type(id, &full_name, w) {
                    Ok(true) => Ok(()),
                    Ok(false) => w.write_str(&full_name),
                    Err(e) => Err(e),
                };
            }
        };

        w.write_str(&full_name)?;
        w.write_char('<')?;
        for (i, param) in generic.iter().enumerate() {
            if i > 0 {
                w.write_str(", ")?;
            }
            w.write_str(&param.name)?;
        }
        w.write_char('>')
    }

    // ─── Instance (narrowed struct view) ───────────────────────────

    /// Writes an Instance type: a class type narrowed by a table literal.
    /// Fields present in the literal have their nil stripped (not optional),
    /// while absent fields retain their original (possibly optional) type.
    fn write_instance_type<W: Write>(&mut self, ins: &LuaInstanceType, w: &mut W) -> fmt::Result {
        let base = ins.get_base();

        // Extract the type decl id from the base type
        let type_id = match base {
            LuaType::Ref(id) | LuaType::Def(id) => id.clone(),
            _ => return self.write_type_inner(base, w),
        };

        let type_decl = match self.db.get_type_index().get_type_decl(&type_id) {
            Some(decl) => decl,
            None => return self.write_type_inner(base, w),
        };

        let name = type_decl.get_full_name().to_string();

        let max_display_count = match self.level.max_display_count() {
            Some(n) => n,
            None => {
                w.write_str(&name)?;
                return Ok(());
            }
        };

        // cycle detection
        if !self.visited.insert(type_id.clone()) {
            w.write_str(&name)?;
            return Ok(());
        }

        // Collect keys present in the table literal, along with their types
        let literal_owner = LuaMemberOwner::Element(ins.get_range().clone());
        let member_index = self.db.get_member_index();
        let literal_keys: HashMap<LuaMemberKey, LuaType> = member_index
            .get_sorted_members(&literal_owner)
            .map(|members| {
                members
                    .iter()
                    .map(|m| {
                        let ty = self
                            .db
                            .get_type_index()
                            .get_type_cache(&m.get_id().into())
                            .map(|tc| tc.as_type().clone())
                            .unwrap_or(LuaType::Any);
                        (m.get_key().clone(), ty)
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Get class members
        let class_owner = LuaMemberOwner::Type(type_id.clone());
        let members = match member_index.get_sorted_members(&class_owner) {
            Some(m) => m,
            None => {
                self.visited.remove(&type_id);
                w.write_str(&name)?;
                return Ok(());
            }
        };

        let mut member_vec = Vec::new();
        let mut function_vec = Vec::new();
        for member in members {
            let member_key = member.get_key();
            let type_cache = self
                .db
                .get_type_index()
                .get_type_cache(&member.get_id().into());
            let type_cache = match type_cache {
                Some(type_cache) => type_cache,
                None => &super::LuaTypeCache::InferType(LuaType::Any),
            };
            if type_cache.is_function() {
                function_vec.push(member_key);
            } else {
                member_vec.push((member_key, type_cache.as_type()));
            }
        }

        if member_vec.is_empty() && function_vec.is_empty() {
            self.visited.remove(&type_id);
            w.write_str(&name)?;
            return Ok(());
        }

        let all_count = member_vec.len() + function_vec.len();

        w.write_str(&name)?;
        w.write_str(" {\n")?;

        let saved = self.level;
        self.level = self.child_level();

        let mut count = 0;
        for (member_key, typ) in &member_vec {
            w.write_str("    ")?;
            if let Some(literal_ty) = literal_keys.get(member_key) {
                if literal_ty.is_nullable() {
                    // Literal value is nil/nullable — show as optional (strip nil from
                    // type, but add `?` to the key to signal it was not concretely set).
                    let without_nil = TypeOps::Remove.apply(self.db, typ, &LuaType::Nil);
                    self.write_optional_member_field(member_key, &without_nil, saved, w)?;
                } else {
                    // Concrete value provided — strip nil and display as required.
                    let without_nil = TypeOps::Remove.apply(self.db, typ, &LuaType::Nil);
                    self.write_table_member_field(member_key, &without_nil, saved, w)?;
                }
            } else if typ.is_nullable() {
                // Optional field not provided: show as "name?: type" (without nil)
                let without_nil = TypeOps::Remove.apply(self.db, typ, &LuaType::Nil);
                self.write_optional_member_field(member_key, &without_nil, saved, w)?;
            } else {
                self.write_table_member_field(member_key, typ, saved, w)?;
            }
            w.write_str(",\n")?;
            count += 1;
            if count >= max_display_count {
                break;
            }
        }
        if count < all_count {
            for function_key in &function_vec {
                w.write_str("    ")?;
                write_member_key_and_separator(function_key, saved, w)?;
                w.write_str("function,\n")?;
                count += 1;
                if count >= max_display_count {
                    break;
                }
            }
        }
        if count >= max_display_count {
            writeln!(w, "    ...(+{})", all_count - max_display_count)?;
        }

        self.level = saved;
        self.visited.remove(&type_id);

        w.write_char('}')?;
        Ok(())
    }

    // ─── Simple (expanded struct view) ──────────────────────────────

    /// Tries to write an expanded view of a named type (struct-like fields).
    /// Returns `Ok(true)` if it wrote something, `Ok(false)` if the caller
    /// should fall back to writing the plain name.
    fn write_simple_type<W: Write>(
        &mut self,
        id: &LuaTypeDeclId,
        name: &str,
        w: &mut W,
    ) -> Result<bool, fmt::Error> {
        let max_display_count = match self.level.max_display_count() {
            Some(n) => n,
            None => {
                w.write_str(name)?;
                return Ok(true);
            }
        };

        // cycle detection
        if !self.visited.insert(id.clone()) {
            w.write_str(name)?;
            return Ok(true);
        }

        let member_owner = LuaMemberOwner::Type(id.clone());
        let member_index = self.db.get_member_index();
        let members = match member_index.get_sorted_members(&member_owner) {
            Some(m) => m,
            None => {
                self.visited.remove(id);
                return Ok(false);
            }
        };

        let mut member_vec = Vec::new();
        let mut function_vec = Vec::new();
        for member in members {
            let member_key = member.get_key();
            let type_cache = self
                .db
                .get_type_index()
                .get_type_cache(&member.get_id().into());
            let type_cache = match type_cache {
                Some(type_cache) => type_cache,
                None => &super::LuaTypeCache::InferType(LuaType::Any),
            };
            if type_cache.is_function() {
                function_vec.push(member_key);
            } else {
                member_vec.push((member_key, type_cache.as_type()));
            }
        }

        if member_vec.is_empty() && function_vec.is_empty() {
            self.visited.remove(id);
            w.write_str(name)?;
            return Ok(true);
        }

        let all_count = member_vec.len() + function_vec.len();

        w.write_str(name)?;
        w.write_str(" {\n")?;

        let saved = self.level;
        self.level = self.child_level();

        let mut count = 0;
        for (member_key, typ) in &member_vec {
            w.write_str("    ")?;
            self.write_table_member_field(member_key, typ, saved, w)?;
            w.write_str(",\n")?;
            count += 1;
            if count >= max_display_count {
                break;
            }
        }
        if count < all_count {
            for function_key in &function_vec {
                w.write_str("    ")?;
                write_member_key_and_separator(function_key, saved, w)?;
                w.write_str("function,\n")?;
                count += 1;
                if count >= max_display_count {
                    break;
                }
            }
        }
        if count >= max_display_count {
            writeln!(w, "    ...(+{})", all_count - max_display_count)?;
        }

        self.level = saved;
        self.visited.remove(id);

        w.write_char('}')?;
        Ok(true)
    }

    // ─── Union ──────────────────────────────────────────────────────

    fn write_union_type<W: Write>(&mut self, union: &LuaUnionType, w: &mut W) -> fmt::Result {
        let types = union.into_vec();
        let num = self.level.max_union_items();

        let saved = self.level;
        self.level = self.child_level();

        // First pass: build dedup keys and collect unique types
        let mut seen = HashSet::new();
        let mut unique_types: Vec<(&LuaType, String)> = Vec::new();
        let mut has_nil = false;
        let mut has_function = false;

        for ty in types.iter() {
            if ty.is_nil() {
                has_nil = true;
                continue;
            } else if ty.is_function() {
                has_function = true;
            }
            let mut key = String::new();
            let _ = self.write_type(ty, &mut key);
            if seen.insert(key.clone()) {
                unique_types.push((ty, key));
            }
        }

        self.level = saved;

        let total = unique_types.len();
        let show_dots = total > num;
        let needs_parens = total > 1 || (total == 1 && has_function && has_nil);

        if needs_parens {
            w.write_char('(')?;
        }

        // Second pass: write unique types directly (reuse cached keys)
        for (i, (_, key)) in unique_types.iter().take(num).enumerate() {
            if i > 0 {
                w.write_char('|')?;
            }
            w.write_str(key)?;
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

    // ─── MultiLineUnion ─────────────────────────────────────────────

    fn write_multi_line_union_type<W: Write>(
        &mut self,
        multi_union: &LuaMultiLineUnion,
        w: &mut W,
    ) -> fmt::Result {
        let members = multi_union.get_unions();
        let num = self.level.max_items();
        let dots = if members.len() > num { "..." } else { "" };

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
        w.write_str(dots)?;
        w.write_char(')')?;

        if saved != RenderLevel::Detailed {
            return Ok(());
        }

        w.write_char('\n')?;
        // detail lines
        let saved_level = self.level;
        self.level = RenderLevel::Minimal;
        for (typ, description) in members {
            w.write_str("    | ")?;
            self.write_type(typ, w)?;
            if let Some(desc) = description {
                w.write_str(" -- ")?;
                w.write_str(&desc.replace('\n', " "))?;
            }
            w.write_char('\n')?;
        }
        self.level = saved_level;
        Ok(())
    }

    // ─── Tuple ──────────────────────────────────────────────────────

    fn write_tuple_type<W: Write>(&mut self, tuple: &LuaTupleType, w: &mut W) -> fmt::Result {
        let types = tuple.get_types();
        let num = self.level.max_items();
        let dots = types.len() > num;

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
        if dots {
            w.write_str("...")?;
        }
        w.write_char(')')
    }

    // ─── Array ──────────────────────────────────────────────────────

    fn write_array_type<W: Write>(&mut self, inner: &LuaType, w: &mut W) -> fmt::Result {
        let saved = self.level;
        self.level = self.child_level();
        self.write_type(inner, w)?;
        self.level = saved;
        w.write_str("[]")
    }

    // ─── Call (alias call) ──────────────────────────────────────────

    fn write_call_type<W: Write>(&mut self, inner: &LuaAliasCallType, w: &mut W) -> fmt::Result {
        let basic = match inner.get_call_kind() {
            LuaAliasCallKind::Sub => "sub",
            LuaAliasCallKind::Add => "add",
            LuaAliasCallKind::KeyOf => "keyof",
            LuaAliasCallKind::Extends => "extends",
            LuaAliasCallKind::Select => "select",
            LuaAliasCallKind::Unpack => "unpack",
            LuaAliasCallKind::Index => "index",
            LuaAliasCallKind::RawGet => "rawget",
            LuaAliasCallKind::Merge => "Merge",
        };
        w.write_str(basic)?;
        w.write_char('<')?;
        let saved = self.level;
        self.level = self.child_level();
        for (i, ty) in inner.get_operands().iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            self.write_type(ty, w)?;
        }
        self.level = saved;
        w.write_char('>')
    }

    // ─── DocFunction ────────────────────────────────────────────────

    fn write_doc_function_type<W: Write>(
        &mut self,
        lua_func: &LuaFunctionType,
        w: &mut W,
    ) -> fmt::Result {
        if self.level == RenderLevel::Minimal {
            return w.write_str("fun(...) -> ...");
        }

        match lua_func.get_async_state() {
            AsyncState::None => w.write_str("fun")?,
            AsyncState::Async => w.write_str("async fun")?,
            AsyncState::Sync => w.write_str("sync fun")?,
        }

        w.write_char('(')?;
        let saved = self.level;
        self.level = self.child_level();
        for (i, param) in lua_func.get_params().iter().enumerate() {
            if i > 0 {
                w.write_str(", ")?;
            }
            w.write_str(&param.0)?;
            if let Some(ty) = &param.1 {
                w.write_str(": ")?;
                self.write_type(ty, w)?;
            }
        }
        self.level = saved;
        w.write_char(')')?;

        let ret_type = lua_func.get_ret();
        let return_nil = match ret_type {
            LuaType::Variadic(variadic) => matches!(variadic.get_type(0), Some(LuaType::Nil)),
            _ => ret_type.is_nil(),
        };

        if return_nil {
            return Ok(());
        }

        w.write_str(" -> ")?;
        let saved = self.level;
        self.level = self.child_level();
        self.write_type(ret_type, w)?;
        self.level = saved;
        Ok(())
    }

    // ─── Object ─────────────────────────────────────────────────────

    fn write_object_type<W: Write>(&mut self, object: &LuaObjectType, w: &mut W) -> fmt::Result {
        if self.level == RenderLevel::Minimal {
            return w.write_str("{...}");
        }

        let num = self.level.max_items();
        let fields = object.get_fields();
        let dots = fields.len() > num;

        w.write_str("{ ")?;
        let saved = self.level;
        self.level = self.child_level();

        for (i, field) in fields
            .iter()
            .sorted_by(|a, b| a.0.cmp(&b.0))
            .take(num)
            .enumerate()
        {
            if i > 0 {
                w.write_str(", ")?;
            }
            match &field.0 {
                LuaMemberKey::Integer(idx) => {
                    write!(w, "[{}]: ", idx)?;
                    self.write_type(field.1, w)?;
                }
                LuaMemberKey::Name(s) => {
                    w.write_str(s)?;
                    w.write_str(": ")?;
                    self.write_type(field.1, w)?;
                }
                LuaMemberKey::None | LuaMemberKey::ExprType(_) => {
                    self.write_type(field.1, w)?;
                }
            }
        }

        // index access
        let access = object.get_index_access();
        if !access.is_empty() {
            if !fields.is_empty() {
                w.write_str(", ")?;
            }
            for (i, (key, value)) in access.iter().enumerate() {
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

        if dots {
            w.write_str(", ...")?;
        }
        w.write_str(" }")
    }

    // ─── Intersection ───────────────────────────────────────────────

    fn write_intersect_type<W: Write>(
        &mut self,
        inter: &LuaIntersectionType,
        w: &mut W,
    ) -> fmt::Result {
        let types = inter.get_types();
        let num = self.level.max_items();
        let dots = types.len() > num;

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
        if dots {
            w.write_str(", ...")?;
        }
        w.write_char(')')
    }

    // ─── Generic ────────────────────────────────────────────────────

    fn write_generic_type<W: Write>(&mut self, generic: &LuaGenericType, w: &mut W) -> fmt::Result {
        let base_id = generic.get_base_type_id();
        let type_decl = match self.db.get_type_index().get_type_decl(&base_id) {
            Some(type_decl) => type_decl,
            None => return w.write_str(base_id.get_name()),
        };

        let full_name = type_decl.get_full_name().to_string();

        // Write base<params>
        w.write_str(&full_name)?;
        w.write_char('<')?;
        let saved = self.level;
        self.level = self.child_level();
        for (i, ty) in generic.get_params().iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            self.write_type(ty, w)?;
        }
        self.level = saved;
        w.write_char('>')?;

        // For detailed+ levels, expand alias origin
        if matches!(
            self.level,
            RenderLevel::Documentation | RenderLevel::CustomDetailed(_) | RenderLevel::Detailed
        ) && type_decl.is_alias()
        {
            // cycle detection for alias expansion
            if !self.visited.insert(base_id.clone()) {
                return Ok(());
            }

            let substitutor = TypeSubstitutor::from_type_array(generic.get_params().clone());
            if let Some(origin_type) = type_decl.get_alias_origin(self.db, Some(&substitutor)) {
                w.write_str(" = ")?;
                let saved = self.level;
                self.level = self.child_level();
                self.write_type(&origin_type, w)?;
                self.level = saved;
            }

            self.visited.remove(&base_id);
        }

        Ok(())
    }

    // ─── TableConst ─────────────────────────────────────────────────

    fn write_table_const_type<W: Write>(
        &mut self,
        member_owned: LuaMemberOwner,
        w: &mut W,
    ) -> fmt::Result {
        match self.level {
            RenderLevel::Detailed | RenderLevel::Simple => {
                if self
                    .write_table_const_detail_or_simple(member_owned, w)
                    .is_ok()
                {
                    Ok(())
                } else {
                    w.write_str("table")
                }
            }
            _ => w.write_str("table"),
        }
    }

    fn write_table_const_detail_or_simple<W: Write>(
        &mut self,
        member_owned: LuaMemberOwner,
        w: &mut W,
    ) -> Result<(), fmt::Error> {
        let member_index = self.db.get_member_index();
        let members = match member_index.get_sorted_members(&member_owned) {
            Some(m) => m,
            None => return Err(fmt::Error),
        };

        let is_detailed = self.level == RenderLevel::Detailed;

        if is_detailed {
            w.write_str("{\n")?;
        } else {
            w.write_str("{ ")?;
        }

        let saved = self.level;
        self.level = self.child_level();

        let mut total_length = 0usize;
        let mut total_line = 0usize;
        let mut first = true;

        for member in members {
            let key = member.get_key();
            let type_cache = self
                .db
                .get_type_index()
                .get_type_cache(&member.get_id().into());
            let type_cache = match type_cache {
                Some(tc) => tc,
                None => &super::LuaTypeCache::InferType(LuaType::Any),
            };

            if is_detailed {
                w.write_str("    ")?;
                self.write_table_member_field(key, type_cache.as_type(), saved, w)?;
                w.write_str(",\n")?;
                total_line += 1;
                if total_line >= 12 {
                    w.write_str("    ...\n")?;
                    break;
                }
            } else {
                // Simple: track character length for truncation
                let mut tmp = String::new();
                self.write_table_member_field(key, type_cache.as_type(), saved, &mut tmp)?;
                let member_string_len = tmp.chars().count();

                if !first {
                    w.write_str(", ")?;
                    total_length += 2;
                }
                first = false;

                total_length += member_string_len;
                w.write_str(&tmp)?;
                if total_length > 54 {
                    w.write_str(", ...")?;
                    break;
                }
            }
        }

        self.level = saved;

        if is_detailed {
            w.write_char('}')
        } else {
            w.write_str(" }")
        }
    }

    // ─── TableGeneric ───────────────────────────────────────────────

    fn write_table_generic_type<W: Write>(&mut self, params: &[LuaType], w: &mut W) -> fmt::Result {
        if self.level == RenderLevel::Minimal {
            return w.write_str("table<...>");
        }

        let num = self.level.max_items();
        let dots = params.len() > num;

        w.write_str("table<")?;
        let saved = self.level;
        self.level = self.child_level();
        for (i, ty) in params.iter().take(num).enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            self.write_type(ty, w)?;
        }
        self.level = saved;
        if dots {
            w.write_str(", ...")?;
        }
        w.write_char('>')
    }

    // ─── StrTplRef ──────────────────────────────────────────────────

    fn write_str_tpl_ref_type<W: Write>(
        &mut self,
        str_tpl: &LuaStringTplType,
        w: &mut W,
    ) -> fmt::Result {
        let prefix = str_tpl.get_prefix();
        if prefix.is_empty() {
            w.write_str(str_tpl.get_name())
        } else {
            write!(w, "{}`{}`", prefix, str_tpl.get_name())
        }
    }

    // ─── Variadic ───────────────────────────────────────────────────

    fn write_variadic_type<W: Write>(&mut self, multi: &VariadicType, w: &mut W) -> fmt::Result {
        match multi {
            VariadicType::Base(base) => {
                self.write_type(base, w)?;
                w.write_str(" ...")
            }
            VariadicType::Multi(types) => {
                if self.level == RenderLevel::Minimal {
                    return w.write_str("multi<...>");
                }

                let max_num = self.level.max_items();
                let dots = types.len() > max_num;

                w.write_char('(')?;
                let saved = self.level;
                self.level = self.child_level();
                for (i, ty) in types.iter().take(max_num).enumerate() {
                    if i > 0 {
                        w.write_char(',')?;
                    }
                    self.write_type(ty, w)?;
                }
                self.level = saved;
                if dots {
                    w.write_str(", ...")?;
                }
                w.write_char(')')
            }
        }
    }

    // ─── Signature ──────────────────────────────────────────────────

    fn write_signature_type<W: Write>(
        &mut self,
        signature_id: &LuaSignatureId,
        w: &mut W,
    ) -> fmt::Result {
        if self.level == RenderLevel::Minimal {
            return w.write_str("fun(...) -> ...");
        }

        let signature = match self.db.get_signature_index().get(signature_id) {
            Some(sig) => sig,
            None => return w.write_str("unknown"),
        };

        // generics
        let generics = &signature.generic_params;
        if !generics.is_empty() {
            w.write_str("fun<")?;
            for (i, gp) in generics.iter().enumerate() {
                if i > 0 {
                    w.write_str(", ")?;
                }
                w.write_str(&gp.name)?;
            }
            w.write_char('>')?;
        } else {
            w.write_str("fun")?;
        }

        w.write_char('(')?;
        let saved = self.level;
        let is_vararg = signature.is_vararg;
        let last_idx = signature.params.len();
        self.level = self.child_level();
        for (i, param) in signature.get_type_params().iter().enumerate() {
            if i > 0 {
                w.write_str(", ")?;
            }
            if i == last_idx - 1 && is_vararg {
                w.write_str("...")?;
            }

            w.write_str(&param.0)?;
            if let Some(ty) = &param.1 {
                w.write_str(": ")?;
                self.write_type(ty, w)?;
            }
        }
        self.level = saved;
        w.write_char(')')?;

        // return type
        let ret_type = signature.get_return_type();
        let return_nil = match ret_type {
            LuaType::Variadic(variadic) => matches!(variadic.get_type(0), Some(LuaType::Nil)),
            _ => ret_type.is_nil(),
        };

        if return_nil {
            return Ok(());
        }

        let rets: Vec<_> = signature.return_docs.iter().collect();
        if rets.is_empty() {
            return Ok(());
        }

        w.write_str(" -> ")?;
        let saved = self.level;
        self.level = self.child_level();
        for (i, ret) in rets.iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            self.write_type(&ret.type_ref, w)?;
        }
        self.level = saved;
        Ok(())
    }

    // ─── Conditional ────────────────────────────────────────────────

    fn write_conditional_type<W: Write>(
        &mut self,
        conditional: &LuaConditionalType,
        w: &mut W,
    ) -> fmt::Result {
        let saved = self.level;
        self.level = self.child_level();
        self.write_type(conditional.get_condition(), w)?;
        w.write_str(" and ")?;
        self.write_type(conditional.get_true_type(), w)?;
        w.write_str(" or ")?;
        self.write_type(conditional.get_false_type(), w)?;
        self.level = saved;
        Ok(())
    }

    // ─── ModuleRef ──────────────────────────────────────────────────

    fn write_module_ref<W: Write>(&mut self, file_id: crate::FileId, w: &mut W) -> fmt::Result {
        if let Some(module_info) = self.db.get_module_index().get_module(file_id) {
            if let Some(export_type) = &module_info.export_type {
                if export_type.is_module_ref() {
                    return w.write_str("module 'recursive'");
                }
                let export_type = export_type.clone();
                self.write_type(&export_type, w)
            } else {
                w.write_str("module 'unknown'")
            }
        } else {
            w.write_str("module 'unknown'")
        }
    }

    /// Write an optional member field as "name?: type" (with ? after name, nil stripped from type).
    fn write_optional_member_field<W: Write>(
        &mut self,
        member_key: &LuaMemberKey,
        ty: &LuaType,
        parent_level: RenderLevel,
        w: &mut W,
    ) -> fmt::Result {
        match member_key {
            LuaMemberKey::Name(name) => {
                w.write_str(name)?;
                w.write_str("?")?;
                let separator = if parent_level == RenderLevel::Detailed {
                    ": "
                } else {
                    " = "
                };
                w.write_str(separator)?;
            }
            _ => {
                write_member_key_and_separator(member_key, parent_level, w)?;
            }
        }
        self.write_type(ty, w)
    }

    // ─── helper: write a table member (key: type) ───────────────────

    fn write_table_member_field<W: Write>(
        &mut self,
        member_key: &LuaMemberKey,
        ty: &LuaType,
        parent_level: RenderLevel,
        w: &mut W,
    ) -> fmt::Result {
        write_member_key_and_separator(member_key, parent_level, w)?;

        if parent_level == RenderLevel::Detailed {
            // Show "integer = 42" style for const types
            match ty {
                LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
                    w.write_str("integer = ")?;
                    self.write_type(ty, w)
                }
                LuaType::FloatConst(_) => {
                    w.write_str("number = ")?;
                    self.write_type(ty, w)
                }
                LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
                    w.write_str("string = ")?;
                    self.write_type(ty, w)
                }
                LuaType::BooleanConst(_) => {
                    w.write_str("boolean = ")?;
                    self.write_type(ty, w)
                }
                _ => self.write_type(ty, w),
            }
        } else {
            self.write_type(ty, w)
        }
    }
}

// ─── Free helper functions ──────────────────────────────────────────────────

fn write_member_key_and_separator<W: Write>(
    member_key: &LuaMemberKey,
    level: RenderLevel,
    w: &mut W,
) -> fmt::Result {
    let separator = if level == RenderLevel::Detailed {
        ": "
    } else {
        " = "
    };
    match member_key {
        LuaMemberKey::Name(name) => {
            w.write_str(name)?;
            w.write_str(separator)
        }
        LuaMemberKey::Integer(i) => {
            write!(w, "[{}]", i)?;
            w.write_str(separator)
        }
        LuaMemberKey::None | LuaMemberKey::ExprType(_) => Ok(()),
    }
}

/// Write an escaped version of `s` directly into `w`.
fn write_hover_escape_string<W: Write>(s: &str, w: &mut W) -> fmt::Result {
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
    Ok(())
}

/// Depth-guard token. Just a marker type; actual depth tracking is done in
/// `TypeHumanizer::guard` / `leave_guard`.
struct DepthGuardToken;

// ─── Public backward-compatible API ─────────────────────────────────────────

/// Humanize a type into a display string. This is the primary backward-compatible
/// entry point. Internally uses `TypeHumanizer` for efficient, depth-bounded rendering.
pub fn humanize_type(db: &DbIndex, ty: &LuaType, level: RenderLevel) -> String {
    let mut humanizer = TypeHumanizer::new(db, level);
    let mut buf = String::new();
    let _ = humanizer.write_type(ty, &mut buf);
    buf
}

/// Format a union type using a custom type formatter closure.
/// This keeps backward compatibility for callers (e.g. inlay hints, hover)
/// that need to inject their own formatting logic per union member.
pub fn format_union_type<F>(
    union: &LuaUnionType,
    level: RenderLevel,
    mut type_formatter: F,
) -> String
where
    F: FnMut(&LuaType, RenderLevel) -> String,
{
    let types = union.into_vec();
    let num = level.max_union_items();

    let mut seen = HashSet::new();
    let mut type_strings = Vec::new();
    let mut has_nil = false;
    let mut has_function = false;
    for ty in types.iter() {
        if ty.is_nil() {
            has_nil = true;
            continue;
        } else if ty.is_function() {
            has_function = true;
        }
        let type_str = type_formatter(ty, level.next_level());
        if seen.insert(type_str.clone()) {
            type_strings.push(type_str);
        }
    }
    let dots = if type_strings.len() > num { "..." } else { "" };
    let display_types: Vec<_> = type_strings.into_iter().take(num).collect();
    let type_str = display_types.join("|");

    if display_types.len() == 1 {
        if has_function && has_nil {
            format!("({})?", type_str)
        } else {
            format!("{}{}", type_str, if has_nil { "?" } else { "" })
        }
    } else {
        format!("({}{}){}", type_str, dots, if has_nil { "?" } else { "" })
    }
}
