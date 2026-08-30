use emmylua_code_analysis::LuaType;

use crate::doc_model::{DocGlobal, DocMember, DocModel, DocModule, DocType};
use crate::html_generator::html_type::{self, TypeLinker};
use crate::html_generator::types::{HtmlDoc, HtmlMember};
use crate::markdown_generator::generator::collect_property;

use super::html_type::{render_const_type_html, render_function_signature_html, render_type_html};
use super::render::{html_escape, signature_pre};

/// Rendering context shared by page generators.
pub struct GenContext<'a> {
    pub model: &'a DocModel,
    /// Maps a type declaration id to a page href (prefixed relative to the
    /// page being generated).
    pub linker: &'a TypeLinker<'a>,
}

pub fn build_type_doc(ctx: &GenContext, doc_type: &DocType) -> Option<HtmlDoc> {
    let typ_name = doc_type.full_name.clone();
    let mut doc = HtmlDoc {
        name: typ_name,
        ..Default::default()
    };

    match doc_type.kind {
        crate::doc_model::DocTypeKind::Class => build_class_doc(ctx, doc_type, &mut doc),
        crate::doc_model::DocTypeKind::Enum => build_enum_doc(ctx, doc_type, &mut doc),
        crate::doc_model::DocTypeKind::Alias => build_alias_doc(ctx, doc_type, &mut doc),
    }
    Some(doc)
}

fn build_class_doc(ctx: &GenContext, doc_type: &DocType, doc: &mut HtmlDoc) {
    let model = ctx.model;
    doc.kind = "class".to_string();
    doc.title = format!("class {}", doc.name);
    if let Some(index) = doc_type.full_name.rfind('.') {
        doc.set_namespace(&doc_type.full_name[..index]);
    }
    doc.set_property(collect_property(&doc_type.property));

    if !doc_type.bases.is_empty() {
        let super_texts: Vec<String> = doc_type
            .bases
            .iter()
            .map(|super_typ| render_type_html(model, super_typ, ctx.linker))
            .collect();
        doc.set_supers(super_texts.join(", "));
    }

    let (methods, fields) = collect_members(ctx, &doc_type.members, &doc_type.name);
    doc.methods = methods;
    doc.fields = fields;
}

fn build_enum_doc(ctx: &GenContext, doc_type: &DocType, doc: &mut HtmlDoc) {
    doc.kind = "enum".to_string();
    doc.title = format!("enum {}", doc.name);
    if let Some(index) = doc_type.full_name.rfind('.') {
        doc.set_namespace(&doc_type.full_name[..index]);
    }
    doc.set_property(collect_property(&doc_type.property));

    let (_, fields) = collect_members(ctx, &doc_type.members, &doc_type.name);
    doc.fields = fields;
}

fn build_alias_doc(ctx: &GenContext, doc_type: &DocType, doc: &mut HtmlDoc) {
    let model = ctx.model;
    doc.kind = "alias".to_string();
    doc.title = format!("alias {}", doc.name);
    if let Some(index) = doc_type.full_name.rfind('.') {
        doc.set_namespace(&doc_type.full_name[..index]);
    }
    doc.set_property(collect_property(&doc_type.property));

    if let Some(origin_typ) = &doc_type.alias_type {
        let is_union = matches!(origin_typ, LuaType::Union(_) | LuaType::MultiLineUnion(_));
        let style = if is_union {
            super::html_type::TypeStyle::Multiline
        } else {
            super::html_type::TypeStyle::Inline
        };
        let origin_type_display =
            super::html_type::render_type_style(model, origin_typ, ctx.linker, style);
        let name = html_escape(&doc.name);
        doc.display = Some(if is_union {
            signature_pre(format!("(alias) {name} =\n{origin_type_display}"))
        } else {
            signature_pre(format!("(alias) {name} = {origin_type_display}"))
        });
    }
}

pub fn build_module_doc(ctx: &GenContext, module: &DocModule) -> Option<HtmlDoc> {
    let mut doc = HtmlDoc {
        kind: "module".to_string(),
        name: module.name.clone(),
        title: format!("module {}", module.name),
        ..Default::default()
    };
    doc.set_property(collect_property(&module.property));

    let members = ctx.model.members_of_type(&module.export_type);
    let (methods, fields) = collect_members(ctx, &members, "M");
    doc.methods = methods;
    doc.fields = fields;
    Some(doc)
}

pub fn build_global_doc(ctx: &GenContext, global: &DocGlobal) -> Option<HtmlDoc> {
    let name = global.name.clone();
    let mut doc = HtmlDoc {
        kind: "global".to_string(),
        name: name.clone(),
        title: format!("global {}", name),
        ..Default::default()
    };
    doc.set_property(collect_property(&global.property));

    match &global.ty {
        LuaType::TableConst(_) | LuaType::Instance(_) => {
            let members = ctx.model.members_of_type(&global.ty);
            let (methods, fields) = collect_members(ctx, &members, &name);
            doc.methods = methods;
            doc.fields = fields;
        }
        _ => build_simple_global(ctx, global, &mut doc),
    }
    Some(doc)
}

fn build_simple_global(ctx: &GenContext, global: &DocGlobal, doc: &mut HtmlDoc) {
    let model = ctx.model;
    let name = &global.name;
    let ty = &global.ty;
    if model.function_info(ty, global.signature.as_ref()).is_some() {
        doc.display = Some(signature_pre(render_function_signature_html(
            model, ty, name, false, ctx.linker,
        )));
    } else if ty.is_const() {
        let typ_display = render_const_type_html(model, ty, ctx.linker);
        doc.display = Some(signature_pre(format!(
            "{}: {}",
            html_escape(name),
            typ_display
        )));
    } else {
        let typ_display = render_type_html(model, ty, ctx.linker);
        doc.display = Some(signature_pre(format!(
            "{} : {}",
            html_escape(name),
            typ_display
        )));
    }
}

/// Renders a field path like `Vector.x` with syntax highlighting
/// (`owner` as variable, `.` as operator, `name` as property).
fn field_name_html(owner: &str, name: &str) -> String {
    format!(
        "<span class=\"hl-var\">{}</span><span class=\"hl-op\">.</span><span class=\"hl-prop\">{}</span>",
        html_escape(owner),
        html_escape(name)
    )
}

/// Collects public methods and fields of a member owner with linked signatures.
pub fn collect_members(
    ctx: &GenContext,
    members: &[DocMember],
    owner_name: &str,
) -> (
    Vec<crate::html_generator::types::HtmlMember>,
    Vec<crate::html_generator::types::HtmlMember>,
) {
    let model = ctx.model;
    let mut methods = Vec::new();
    let mut fields = Vec::new();

    for member in members {
        if member
            .property
            .visibility
            .is_some_and(|visibility| visibility != emmylua_parser::VisibilityKind::Public)
        {
            continue;
        }

        let title_name = format!("{}.{}", owner_name, member.name);
        let property = collect_property(&member.property);

        if let Some(func) = model.function_info(&member.ty, member.signature.as_ref()) {
            let display = signature_pre(render_function_signature_html(
                model,
                &member.ty,
                &title_name,
                false,
                ctx.linker,
            ));
            let mut html_member = HtmlMember::from_property(title_name.clone(), display, property);
            html_member.params = func
                .params
                .iter()
                .map(|param| super::types::HtmlParam {
                    name: param.name.clone(),
                    type_html: param
                        .ty
                        .as_ref()
                        .map(|ty| render_type_html(model, ty, ctx.linker))
                        .unwrap_or_default(),
                    description: None,
                })
                .collect();
            html_member.returns = func
                .returns
                .iter()
                .map(|ty| super::types::HtmlParam {
                    name: String::new(),
                    type_html: render_type_html(model, ty, ctx.linker),
                    description: None,
                })
                .collect();
            html_member.overloads = html_type::signature_overloads_html(
                model,
                &func.overloads,
                &title_name,
                ctx.linker,
            )
            .into_iter()
            .map(signature_pre)
            .collect();
            methods.push(html_member);
        } else if member.ty.is_const() {
            let const_type_display = render_const_type_html(model, &member.ty, ctx.linker);
            let display = signature_pre(format!(
                "{}<span class=\"hl-op\">:</span> {}",
                field_name_html(owner_name, &member.name),
                const_type_display
            ));
            fields.push(HtmlMember::from_property(title_name, display, property));
        } else {
            let typ_display = render_type_html(model, &member.ty, ctx.linker);
            let display = signature_pre(format!(
                "{} <span class=\"hl-op\">:</span> {}",
                field_name_html(owner_name, &member.name),
                typ_display
            ));
            fields.push(HtmlMember::from_property(title_name, display, property));
        }
    }

    (methods, fields)
}
