use emmylua_code_analysis::{
    DbIndex, LuaDecl, LuaDeclId, LuaMemberKey, LuaMemberOwner, LuaSemanticDeclId, LuaType,
    LuaTypeCache, LuaTypeDecl, LuaTypeDeclId, ModuleInfo,
};
use emmylua_parser::VisibilityKind;

use crate::html_generator::types::HtmlDoc;
use crate::markdown_generator::generator::collect_property;

use super::html_type::{render_const_type_html, render_function_signature_html, render_type_html};
use super::render::{html_escape, signature_pre};

/// Rendering context shared by page generators.
pub struct GenContext<'a> {
    pub db: &'a DbIndex,
    /// Maps a type declaration id to a page href (prefixed relative to the
    /// page being generated).
    pub linker: &'a dyn Fn(&LuaTypeDeclId) -> Option<String>,
}

pub fn build_type_doc(ctx: &GenContext, typ: &LuaTypeDecl) -> Option<HtmlDoc> {
    let typ_name = typ.get_full_name().to_string();
    let mut doc = HtmlDoc {
        name: typ_name,
        ..Default::default()
    };

    if typ.is_class() {
        build_class_doc(ctx, typ, &mut doc);
    } else if typ.is_enum() {
        build_enum_doc(ctx, typ, &mut doc);
    } else {
        build_alias_doc(ctx, typ, &mut doc);
    }
    Some(doc)
}

fn build_class_doc(ctx: &GenContext, typ: &LuaTypeDecl, doc: &mut HtmlDoc) {
    let db = ctx.db;
    doc.kind = "class".to_string();
    doc.title = format!("class {}", doc.name);
    let typ_id = typ.get_id();
    if let Some(namespace) = typ.get_namespace() {
        doc.set_namespace(namespace);
    }
    doc.set_property(collect_property(
        db,
        LuaSemanticDeclId::TypeDecl(typ_id.clone()),
    ));

    if let Some(supers) = db.get_type_index().get_super_types(&typ_id) {
        let super_texts: Vec<String> = supers
            .iter()
            .map(|super_typ| render_type_html(db, super_typ, ctx.linker))
            .collect();
        if !super_texts.is_empty() {
            doc.set_supers(super_texts.join(", "));
        }
    }

    let (methods, fields) = collect_members(ctx, &LuaMemberOwner::Type(typ_id), typ.get_name());
    doc.methods = methods;
    doc.fields = fields;
}

fn build_enum_doc(ctx: &GenContext, typ: &LuaTypeDecl, doc: &mut HtmlDoc) {
    let db = ctx.db;
    doc.kind = "enum".to_string();
    doc.title = format!("enum {}", doc.name);
    let typ_id = typ.get_id();
    if let Some(namespace) = typ.get_namespace() {
        doc.set_namespace(namespace);
    }
    doc.set_property(collect_property(
        db,
        LuaSemanticDeclId::TypeDecl(typ_id.clone()),
    ));

    let (_, fields) = collect_members(ctx, &LuaMemberOwner::Type(typ_id), typ.get_name());
    doc.fields = fields;
}

fn build_alias_doc(ctx: &GenContext, typ: &LuaTypeDecl, doc: &mut HtmlDoc) {
    let db = ctx.db;
    doc.kind = "alias".to_string();
    doc.title = format!("alias {}", doc.name);
    if let Some(namespace) = typ.get_namespace() {
        doc.set_namespace(namespace);
    }
    doc.set_property(collect_property(
        db,
        LuaSemanticDeclId::TypeDecl(typ.get_id().clone()),
    ));

    if let Some(origin_typ) = typ.get_alias_origin(db, None) {
        let is_union = matches!(origin_typ, LuaType::Union(_) | LuaType::MultiLineUnion(_));
        let style = if is_union {
            super::html_type::TypeStyle::Multiline
        } else {
            super::html_type::TypeStyle::Inline
        };
        let origin_type_display =
            super::html_type::render_type_style(db, &origin_typ, ctx.linker, style);
        let name = html_escape(&doc.name);
        doc.display = Some(if is_union {
            signature_pre(format!("(alias) {name} =\n{origin_type_display}"))
        } else {
            signature_pre(format!("(alias) {name} = {origin_type_display}"))
        });
    }
}

pub fn build_module_doc(ctx: &GenContext, module: &ModuleInfo) -> Option<HtmlDoc> {
    let db = ctx.db;
    let mut doc = HtmlDoc {
        kind: "module".to_string(),
        name: module.full_module_name.clone(),
        title: format!("module {}", module.full_module_name),
        ..Default::default()
    };
    if let Some(property_id) = &module.semantic_id {
        doc.set_property(collect_property(db, property_id.clone()));
    }

    let export_typ = module.export_type.clone()?;
    match &export_typ {
        LuaType::Def(type_id) => {
            let owner = LuaMemberOwner::Type(type_id.clone());
            let (methods, fields) = collect_members(ctx, &owner, type_id.get_simple_name());
            doc.methods = methods;
            doc.fields = fields;
        }
        LuaType::TableConst(t) => {
            let owner = LuaMemberOwner::Element(t.clone());
            let (methods, fields) = collect_members(ctx, &owner, "M");
            doc.methods = methods;
            doc.fields = fields;
        }
        LuaType::Instance(i) => {
            let owner = LuaMemberOwner::Element(i.get_range().clone());
            let (methods, fields) = collect_members(ctx, &owner, "M");
            doc.methods = methods;
            doc.fields = fields;
        }
        _ => {}
    }
    Some(doc)
}

pub fn build_global_doc(ctx: &GenContext, decl_id: &LuaDeclId) -> Option<HtmlDoc> {
    let db = ctx.db;
    let decl = db.get_decl_index().get_decl(decl_id)?;
    let name = decl.get_name();
    let mut doc = HtmlDoc {
        kind: "global".to_string(),
        name: name.to_string(),
        title: format!("global {}", name),
        ..Default::default()
    };
    doc.set_property(collect_property(
        db,
        LuaSemanticDeclId::LuaDecl(decl.get_id()),
    ));

    let decl_type = db.get_type_index().get_type_cache(&(*decl_id).into())?;
    match decl_type.as_type() {
        LuaType::TableConst(table) => {
            let owner = LuaMemberOwner::Element(table.clone());
            let (methods, fields) = collect_members(ctx, &owner, name);
            doc.methods = methods;
            doc.fields = fields;
        }
        _ => {
            build_simple_global(ctx, decl, &mut doc);
        }
    }
    Some(doc)
}

fn build_simple_global(ctx: &GenContext, decl: &LuaDecl, doc: &mut HtmlDoc) {
    let db = ctx.db;
    let name = decl.get_name();
    let Some(ty) = db.get_type_index().get_type_cache(&decl.get_id().into()) else {
        return;
    };
    if ty.is_function() {
        doc.display = Some(signature_pre(render_function_signature_html(
            db, ty, name, false, ctx.linker,
        )));
    } else if ty.is_const() {
        let typ_display = render_const_type_html(db, ty, ctx.linker);
        doc.display = Some(signature_pre(format!(
            "{}: {}",
            html_escape(name),
            typ_display
        )));
    } else {
        let typ_display = render_type_html(db, ty, ctx.linker);
        doc.display = Some(signature_pre(format!(
            "{} : {}",
            html_escape(name),
            typ_display
        )));
    }
}

/// Collects public methods and fields of a member owner with linked signatures.
pub fn collect_members(
    ctx: &GenContext,
    member_owner: &LuaMemberOwner,
    owner_name: &str,
) -> (
    Vec<crate::html_generator::types::HtmlMember>,
    Vec<crate::html_generator::types::HtmlMember>,
) {
    let db = ctx.db;
    let mut methods = Vec::new();
    let mut fields = Vec::new();
    let Some(members) = db.get_member_index().get_sorted_members(member_owner) else {
        return (methods, fields);
    };

    for member in members {
        let member_type = db
            .get_type_index()
            .get_type_cache(&member.get_id().into())
            .unwrap_or(&LuaTypeCache::InferType(LuaType::Unknown))
            .as_type();
        let member_id = member.get_id();
        let member_property_id = LuaSemanticDeclId::Member(member_id);
        let property = db.get_property_index().get_property(&member_property_id);
        if let Some(property) = property
            && property.visibility != VisibilityKind::Public
        {
            continue;
        }

        let member_property = collect_property(db, member_property_id);
        let member_key = member.get_key();
        let name = match member_key {
            LuaMemberKey::Name(name) => name.to_string(),
            LuaMemberKey::Integer(i) => format!("[{}]", i),
            _ => continue,
        };
        let title_name = format!("{}.{}", owner_name, name);

        if member_type.is_function() {
            let display = signature_pre(render_function_signature_html(
                db,
                member_type,
                &format!("{}.{}", owner_name, name),
                false,
                ctx.linker,
            ));
            let mut member = crate::html_generator::types::HtmlMember::from_property(
                title_name,
                display,
                member_property,
            );
            if let Some((params, returns)) =
                super::html_type::function_details_html(db, member_type, ctx.linker)
            {
                member.params = params;
                member.returns = returns;
            }
            member.overloads = super::html_type::signature_overloads_html(
                db,
                member_type,
                &format!("{}.{}", owner_name, name),
                ctx.linker,
            )
            .into_iter()
            .map(signature_pre)
            .collect();
            methods.push(member);
        } else if member_type.is_const() {
            let const_type_display = render_const_type_html(db, member_type, ctx.linker);
            let display = signature_pre(format!(
                "{}.{}: {}",
                html_escape(owner_name),
                html_escape(&name),
                const_type_display
            ));
            fields.push(crate::html_generator::types::HtmlMember::from_property(
                title_name,
                display,
                member_property,
            ));
        } else {
            let typ_display = render_type_html(db, member_type, ctx.linker);
            let display = signature_pre(format!(
                "{}.{} : {}",
                html_escape(owner_name),
                html_escape(&name),
                typ_display
            ));
            fields.push(crate::html_generator::types::HtmlMember::from_property(
                title_name,
                display,
                member_property,
            ));
        }
    }

    (methods, fields)
}
