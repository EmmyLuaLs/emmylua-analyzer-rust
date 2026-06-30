//! Salsa-native TypeHumanizer — full-featured humanize_type replacement.
//!
//! Uses `&SalsaSummaryDatabase` + `FileId` instead of `&DbIndex`.

use std::collections::HashSet;
use std::fmt::{self, Write};

use smol_str::SmolStr;

use crate::compilation::{
    SalsaDocTypeDefKindSummary, SalsaDocTypeLoweredKind, SalsaPropertyKeySummary,
    SalsaSummaryDatabase,
};
use crate::{FileId, LuaType, LuaTypeDeclId, RenderLevel};

const MAX_HUMANIZE_DEPTH: u8 = 3;

/// Full salsa-native humanize_type.
pub fn humanize_type_salsa(
    db: &SalsaSummaryDatabase,
    file_id: FileId,
    ty: &LuaType,
    level: RenderLevel,
) -> String {
    let mut h = TypeHumanizerSalsa {
        db,
        file_id,
        level,
        depth: 0,
        max_depth: MAX_HUMANIZE_DEPTH,
        visited: HashSet::new(),
    };
    let mut buf = String::new();
    let _ = h.write_type(ty, &mut buf);
    buf
}

struct TypeHumanizerSalsa<'a> {
    db: &'a SalsaSummaryDatabase,
    file_id: FileId,
    level: RenderLevel,
    depth: u8,
    max_depth: u8,
    visited: HashSet<LuaTypeDeclId>,
}

impl<'a> TypeHumanizerSalsa<'a> {
    fn guard(&mut self) -> bool {
        if self.depth >= self.max_depth {
            return false;
        }
        self.depth += 1;
        true
    }
    fn unguard(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
    fn child_level(&self) -> RenderLevel {
        self.level.next_level()
    }
    fn is_detailed(&self) -> bool {
        matches!(
            self.level,
            RenderLevel::Detailed | RenderLevel::Documentation
        )
    }
    fn is_brief(&self) -> bool {
        matches!(
            self.level,
            RenderLevel::Brief | RenderLevel::Minimal | RenderLevel::Simple
        )
    }

    fn write_type(&mut self, ty: &LuaType, w: &mut dyn Write) -> fmt::Result {
        self.write_type_inner(ty, w)
    }

    fn write_type_inner(&mut self, ty: &LuaType, w: &mut dyn Write) -> fmt::Result {
        match ty {
            LuaType::Unknown => w.write_str("unknown"),
            LuaType::Any => w.write_str("any"),
            LuaType::Nil => w.write_str("nil"),
            LuaType::Never => w.write_str("never"),
            LuaType::Boolean | LuaType::BooleanConst(_) | LuaType::DocBooleanConst(_) => {
                w.write_str("boolean")
            }
            LuaType::Integer | LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => {
                w.write_str("integer")
            }
            LuaType::Number | LuaType::FloatConst(_) => w.write_str("number"),
            LuaType::String | LuaType::StringConst(_) | LuaType::DocStringConst(_) => {
                w.write_str("string")
            }
            LuaType::Function => w.write_str("fun"),
            LuaType::Table => w.write_str("table"),
            LuaType::TableConst(_) => self.write_table_const(ty, w),
            LuaType::TableGeneric(params) => self.write_table_generic(params, w),
            LuaType::Thread => w.write_str("thread"),
            LuaType::Userdata => w.write_str("userdata"),
            LuaType::Global => w.write_str("_G"),
            LuaType::Io => w.write_str("io"),
            LuaType::Language(_) => w.write_str("language"),

            LuaType::Ref(id) => self.write_ref(id, w),
            LuaType::Def(id) => self.write_def(id, w),
            LuaType::Union(u) => self.write_union(u, w),
            LuaType::Intersection(i) => self.write_intersection(i, w),
            LuaType::Array(a) => {
                self.write_type(a.get_base(), w)?;
                w.write_str("[]")
            }
            LuaType::Tuple(t) => {
                w.write_char('(')?;
                for (i, ty) in t.get_types().iter().enumerate() {
                    if i > 0 {
                        w.write_str(", ")?;
                    }
                    self.write_type(ty, w)?;
                }
                w.write_char(')')
            }
            LuaType::Variadic(v) => self.write_variadic(v, w),
            LuaType::Generic(g) => self.write_generic(g, w),
            LuaType::DocFunction(f) => self.write_doc_function(f, w),
            LuaType::Signature(_) => w.write_str("fun"),
            LuaType::Object(o) => self.write_object(o, w),
            LuaType::DocStringConst(s) => write!(w, "\"{}\"", s),
            LuaType::StringConst(s) => write!(w, "\"{}\"", s),
            LuaType::IntegerConst(n) => write!(w, "{}", n),
            LuaType::FloatConst(n) => write!(w, "{}", n),
            LuaType::BooleanConst(b) => write!(w, "{}", b),
            LuaType::DocIntegerConst(n) => write!(w, "{}", n),
            LuaType::DocBooleanConst(b) => write!(w, "{}", b),
            LuaType::TplRef(_) => w.write_str("T"),
            LuaType::StrTplRef(_) => w.write_str("`T`"),
            LuaType::Instance(i) => self.write_type(i.get_base(), w),
            LuaType::ModuleRef(_) => w.write_str("module"),
            LuaType::Namespace(_) => w.write_str("namespace"),
            _ => w.write_str("?"),
        }
    }

    fn write_ref(&mut self, id: &LuaTypeDeclId, w: &mut dyn Write) -> fmt::Result {
        let name = id.get_name();
        let Some(def) = self.find_type_def(name) else {
            return w.write_str(name);
        };

        match def.kind {
            SalsaDocTypeDefKindSummary::Alias => {
                w.write_str("alias(")?;
                w.write_str(name)?;
                w.write_char(')')
            }
            SalsaDocTypeDefKindSummary::Enum => {
                w.write_str("enum(")?;
                w.write_str(name)?;
                w.write_char(')')
            }
            SalsaDocTypeDefKindSummary::Class | SalsaDocTypeDefKindSummary::Attribute => {
                if self.is_detailed() {
                    self.write_class_detail(&def, name, w)
                } else {
                    w.write_str(name)
                }
            }
        }
    }

    fn write_class_detail(
        &mut self,
        def: &crate::compilation::SalsaDocTypeDefSummary,
        name: &str,
        w: &mut dyn Write,
    ) -> fmt::Result {
        w.write_str(name)?;
        // super types
        if !def.super_type_offsets.is_empty() {
            w.write_str(" : ")?;
            for (i, offset) in def.super_type_offsets.iter().enumerate() {
                if i > 0 {
                    w.write_str(", ")?;
                }
                if let Some(resolved) = self.db.doc().resolved_type_by_key(self.file_id, *offset) {
                    let name = match &resolved.lowered.kind {
                        crate::compilation::SalsaDocTypeLoweredKind::Name { name } => {
                            name.to_string()
                        }
                        _ => continue,
                    };
                    w.write_str(&name)?;
                }
            }
        }
        // fields
        if let Some(entries) = self.db.semantic().properties_of_type(name) {
            let mut fields: Vec<&str> = entries
                .iter()
                .filter_map(|e| match &e.key {
                    SalsaPropertyKeySummary::Name(n) => Some(n.as_str()),
                    _ => None,
                })
                .collect();
            if !fields.is_empty() {
                fields.sort();
                w.write_str(" { ")?;
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        w.write_str(", ")?;
                    }
                    w.write_str(f)?;
                }
                w.write_str(" }")?;
            }
        }
        Ok(())
    }

    fn write_def(&mut self, id: &LuaTypeDeclId, w: &mut dyn Write) -> fmt::Result {
        let name = id.get_name();
        if let Some(def) = self.find_type_def(name) {
            if let Some(offset) = def.value_type_offset {
                if let Some(resolved) = self.db.doc().resolved_type_by_key(self.file_id, offset) {
                    if let crate::compilation::SalsaDocTypeLoweredKind::Name { name } =
                        &resolved.lowered.kind
                    {
                        return w.write_str(name.as_str());
                    }
                }
            }
        }
        w.write_str(name)
    }

    fn write_union(&mut self, u: &crate::LuaUnionType, w: &mut dyn Write) -> fmt::Result {
        let types = u.into_vec();
        let max = 5;
        let mut items: Vec<String> = Vec::new();
        for ty in types.iter().take(max) {
            let mut s = String::new();
            self.write_type(ty, &mut s)?;
            if !items.contains(&s) {
                items.push(s);
            }
        }
        if items.is_empty() {
            return w.write_str("?");
        }
        if items.len() == 1 {
            w.write_str(&items[0])
        } else {
            w.write_char('(')?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    w.write_str(" | ")?;
                }
                w.write_str(item)?;
            }
            if types.len() > max {
                w.write_str(" | ...")?;
            }
            w.write_char(')')
        }
    }

    fn write_intersection(
        &mut self,
        i: &crate::LuaIntersectionType,
        w: &mut dyn Write,
    ) -> fmt::Result {
        for (idx, ty) in i.get_types().iter().enumerate() {
            if idx > 0 {
                w.write_str(" & ")?;
            }
            self.write_type(ty, w)?;
        }
        Ok(())
    }

    fn write_table_const(&mut self, _ty: &LuaType, w: &mut dyn Write) -> fmt::Result {
        w.write_str("{}")
    }

    fn write_table_generic(&mut self, params: &[LuaType], w: &mut dyn Write) -> fmt::Result {
        if params.len() >= 2 {
            w.write_str("table<")?;
            self.write_type(&params[0], w)?;
            w.write_str(", ")?;
            self.write_type(&params[1], w)?;
            w.write_char('>')
        } else {
            w.write_str("table")
        }
    }

    fn write_object(&mut self, o: &crate::LuaObjectType, w: &mut dyn Write) -> fmt::Result {
        let fields: Vec<_> = o
            .get_fields()
            .iter()
            .take(if self.is_detailed() { 20 } else { 5 })
            .collect();
        w.write_char('{')?;
        for (i, (key, ty)) in fields.iter().enumerate() {
            if i > 0 {
                w.write_str(", ")?;
            }
            match key {
                crate::LuaMemberKey::Name(n) => write!(w, "{}: ", n)?,
                crate::LuaMemberKey::Integer(n) => write!(w, "[{}]: ", n)?,
                _ => write!(w, "?: ")?,
            }
            self.write_type(ty, w)?;
        }
        w.write_char('}')
    }

    fn write_variadic(&mut self, v: &crate::VariadicType, w: &mut dyn Write) -> fmt::Result {
        match &v {
            crate::VariadicType::Base(b) => {
                self.write_type(b, w)?;
                w.write_str("...")
            }
            crate::VariadicType::Multi(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        w.write_str(", ")?;
                    }
                    self.write_type(t, w)?;
                }
                Ok(())
            }
        }
    }

    fn write_generic(&mut self, g: &crate::LuaGenericType, w: &mut dyn Write) -> fmt::Result {
        self.write_type(&g.get_base_type(), w)?;
        let params = g.get_params();
        if !params.is_empty() {
            w.write_char('<')?;
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    w.write_str(", ")?;
                }
                self.write_type(p, w)?;
            }
            w.write_char('>')?;
        }
        Ok(())
    }

    fn write_doc_function(&mut self, f: &crate::LuaFunctionType, w: &mut dyn Write) -> fmt::Result {
        w.write_str("fun(")?;
        let params = f.get_params();
        for (i, (name, ty)) in params.iter().enumerate() {
            if i > 0 {
                w.write_str(", ")?;
            }
            if let Some(ty) = ty {
                write!(w, "{}: ", name)?;
                self.write_type(ty, w)?;
            } else {
                w.write_str(name)?;
            }
        }
        w.write_char(')')?;
        let ret = f.get_ret();
        if !ret.is_nil() && !ret.is_unknown() {
            w.write_str(" -> ")?;
            self.write_type(ret, w)?;
        }
        Ok(())
    }

    fn find_type_def(&self, name: &str) -> Option<crate::compilation::SalsaDocTypeDefSummary> {
        if let Some(def) = self.db.doc().type_def_by_name(self.file_id, name) {
            return Some(def);
        }
        for fid in self.db.file_ids() {
            if let Some(def) = self.db.doc().type_def_by_name(fid, name) {
                return Some(def);
            }
        }
        None
    }
}
