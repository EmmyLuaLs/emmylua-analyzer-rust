use emmylua_code_analysis::{
    DbIndex, GenericParam, LuaSignatureId, LuaType, LuaTypeDeclId, RenderLevel, humanize_type,
};

use super::markdown::render_markdown;
use super::render::html_escape;
use super::types::HtmlParam;

/// A callback that maps a type declaration id to a page href, or `None` when
/// the type is not documented.
pub type TypeLinker<'a> = dyn Fn(&LuaTypeDeclId) -> Option<String> + 'a;

/// How unions and multi-line unions are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeStyle {
    /// `A | B | C` on a single line.
    Inline,
    /// Each union member on its own line.
    Multiline,
}

/// Recursively renders a `LuaType` as HTML in inline style, linking documented
/// type names.
pub fn render_type_html(db: &DbIndex, typ: &LuaType, linker: &TypeLinker) -> String {
    render_type_style(db, typ, linker, TypeStyle::Inline)
}

/// Recursively renders a `LuaType` as HTML with the given layout style.
pub fn render_type_style(
    db: &DbIndex,
    typ: &LuaType,
    linker: &TypeLinker,
    style: TypeStyle,
) -> String {
    match typ {
        // ─── primitives ─────────────────────────────────────────────
        LuaType::Unknown => type_kw("unknown"),
        LuaType::Any => type_kw("any"),
        LuaType::Nil => type_kw("nil"),
        LuaType::Table => type_kw("table"),
        LuaType::Userdata => type_kw("userdata"),
        LuaType::Function => type_kw("function"),
        LuaType::Thread => type_kw("thread"),
        LuaType::Boolean => type_kw("boolean"),
        LuaType::String => type_kw("string"),
        LuaType::Integer => type_kw("integer"),
        LuaType::Number => type_kw("number"),
        LuaType::Io => type_kw("io"),
        LuaType::SelfInfer => type_kw("self"),
        LuaType::Global => type_kw("global"),
        LuaType::Never => type_kw("never"),
        LuaType::Language(s) => html_escape(s),

        // ─── constants ──────────────────────────────────────────────
        LuaType::BooleanConst(b) | LuaType::DocBooleanConst(b) => {
            format!("<span class=\"hl-kw\">{b}</span>")
        }
        LuaType::StringConst(s) | LuaType::DocStringConst(s) => {
            format!("<span class=\"hl-str\">\"{}\"</span>", html_escape(s))
        }
        LuaType::IntegerConst(i) | LuaType::DocIntegerConst(i) => {
            format!("<span class=\"hl-num\">{i}</span>")
        }
        LuaType::FloatConst(f) => format!("<span class=\"hl-num\">{}</span>", format_float(*f)),

        // ─── named references → links ───────────────────────────────
        LuaType::Ref(id) | LuaType::Def(id) => render_named(db, id, linker),

        // ─── composite ──────────────────────────────────────────────
        LuaType::Array(arr) => format!(
            "{}[]",
            render_type_style(db, arr.get_base(), linker, TypeStyle::Inline)
        ),
        LuaType::Tuple(tuple) => {
            let parts: Vec<String> = tuple
                .get_types()
                .iter()
                .map(|t| render_type_style(db, t, linker, TypeStyle::Inline))
                .collect();
            format!("({})", parts.join(", "))
        }
        LuaType::Union(union) => {
            let parts: Vec<String> = union
                .into_vec()
                .iter()
                .map(|t| render_type_style(db, t, linker, TypeStyle::Inline))
                .collect();
            if style == TypeStyle::Multiline {
                parts
                    .iter()
                    .map(|part| format!("| {part}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                parts.join(" | ")
            }
        }
        LuaType::MultiLineUnion(multi_union) => {
            let parts: Vec<String> = multi_union
                .get_unions()
                .iter()
                .map(|(t, description)| {
                    let body = render_type_style(db, t, linker, TypeStyle::Inline);
                    match description {
                        Some(desc) => format!("{} # {}", body, html_escape(desc)),
                        None => body,
                    }
                })
                .collect();
            if style == TypeStyle::Multiline {
                parts
                    .iter()
                    .map(|part| format!("| {part}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                parts.join(" | ")
            }
        }
        LuaType::Intersection(intersection) => {
            let parts: Vec<String> = intersection
                .get_types()
                .iter()
                .map(|t| render_type_style(db, t, linker, TypeStyle::Inline))
                .collect();
            parts.join(" & ")
        }
        LuaType::DocFunction(func) => {
            render_function_html(db, func.get_params(), Some(func.get_ret()), linker)
        }
        LuaType::Signature(signature_id) => render_signature_html(db, *signature_id, linker),
        LuaType::Generic(generic) => {
            let base = render_named(db, &generic.get_base_type_id(), linker);
            let params: Vec<String> = generic
                .get_params()
                .iter()
                .map(|t| render_type_style(db, t, linker, TypeStyle::Inline))
                .collect();
            format!("{}&lt;{}&gt;", base, params.join(", "))
        }
        LuaType::TableGeneric(params) => {
            let parts: Vec<String> = params
                .iter()
                .map(|t| render_type_style(db, t, linker, TypeStyle::Inline))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        LuaType::TplRef(tpl) => {
            format!(
                "<span class=\"hl-type\">{}</span>",
                html_escape(tpl.get_name())
            )
        }
        LuaType::Variadic(variadic) => match variadic.get_type(0) {
            Some(inner) => format!(
                "...{}",
                render_type_style(db, inner, linker, TypeStyle::Inline)
            ),
            None => "...".to_string(),
        },
        LuaType::Instance(instance) => {
            render_type_style(db, instance.get_base(), linker, TypeStyle::Inline)
        }
        LuaType::Namespace(ns) => format!("{{ {} }}", html_escape(ns)),
        LuaType::ModuleRef(file_id) => db
            .get_module_index()
            .get_module(*file_id)
            .map(|m| html_escape(&m.full_module_name))
            .unwrap_or_else(|| "module".to_string()),

        // ─── complex / exotic → escaped humanized fallback ───────────
        LuaType::TableConst(_)
        | LuaType::Object(_)
        | LuaType::StrTplRef(_)
        | LuaType::Call(_)
        | LuaType::TypeGuard(_)
        | LuaType::Conditional(_)
        | LuaType::Mapped(_) => fallback(db, typ, linker),
    }
}

fn render_named(db: &DbIndex, id: &LuaTypeDeclId, linker: &TypeLinker) -> String {
    let name = db
        .get_type_index()
        .get_type_decl(id)
        .map(|decl| decl.get_full_name().to_string())
        .unwrap_or_else(|| id.get_name().to_string());
    if let Some(href) = linker(id) {
        format!(
            "<a href=\"{}\">{}</a>",
            html_escape(&href),
            html_escape(&name)
        )
    } else {
        // No documentation page for this type — render it as a type name.
        format!("<span class=\"hl-type\">{}</span>", html_escape(&name))
    }
}

/// Renders a constant type with its value.
pub fn render_const_type_html(db: &DbIndex, typ: &LuaType, linker: &TypeLinker) -> String {
    let value = render_type_html(db, typ, linker);
    match typ {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => format!("integer = {value}"),
        LuaType::FloatConst(_) => format!("number = {value}"),
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => format!("string = {value}"),
        _ => value,
    }
}

/// A function's parameter and return-value rows.
pub type FunctionDetails = (Vec<HtmlParam>, Vec<HtmlParam>);

/// Extracts parameter and return-value rows (with descriptions) for a function
/// type, used to render detail tables under a method signature.
///
/// Returns `None` for non-function types. Rows are only included when at least
/// one parameter or return value carries a description, so empty tables are not
/// emitted.
pub fn function_details_html(
    db: &DbIndex,
    typ: &LuaType,
    linker: &TypeLinker,
) -> Option<FunctionDetails> {
    let (params, returns) = match typ {
        LuaType::Signature(signature_id) => {
            let signature = db.get_signature_index().get(signature_id)?;
            let params = signature
                .get_type_params()
                .iter()
                .enumerate()
                .map(|(idx, (name, ty))| HtmlParam {
                    name: name.clone(),
                    type_html: ty
                        .as_ref()
                        .map(|t| render_type_html(db, t, linker))
                        .unwrap_or_default(),
                    description: signature
                        .get_param_info_by_id(idx)
                        .and_then(|info| info.description.clone())
                        .map(|d| render_markdown(&d)),
                })
                .collect::<Vec<_>>();
            let returns = signature
                .return_docs
                .iter()
                .map(|ret| HtmlParam {
                    name: ret.name.clone().unwrap_or_default(),
                    type_html: render_type_html(db, &ret.type_ref, linker),
                    description: ret.description.clone().map(|d| render_markdown(&d)),
                })
                .collect::<Vec<_>>();
            (params, returns)
        }
        LuaType::DocFunction(func) => {
            let params = func
                .get_params()
                .iter()
                .map(|(name, ty)| HtmlParam {
                    name: name.clone(),
                    type_html: ty
                        .as_ref()
                        .map(|t| render_type_html(db, t, linker))
                        .unwrap_or_default(),
                    description: None,
                })
                .collect::<Vec<_>>();
            let returns = vec![HtmlParam {
                name: String::new(),
                type_html: render_type_html(db, func.get_ret(), linker),
                description: None,
            }];
            (params, returns)
        }
        _ => return None,
    };

    let has_description = params.iter().any(|p| p.description.is_some())
        || returns.iter().any(|r| r.description.is_some());
    has_description.then_some((params, returns))
}

/// Wraps a Lua keyword in a syntax-highlight span.
fn kw(text: &str) -> String {
    format!("<span class=\"hl-kw\">{text}</span>")
}

/// Wraps a primitive type name in a syntax-highlight span.
fn type_kw(text: &str) -> String {
    kw(text)
}

/// Wraps a function/method name in a syntax-highlight span.
fn fn_name(text: &str) -> String {
    format!("<span class=\"hl-fn\">{}</span>", html_escape(text))
}

/// Wraps a parameter name in a syntax-highlight span.
fn param_name(text: &str) -> String {
    format!("<span class=\"hl-var\">{}</span>", html_escape(text))
}

/// Renders a compact `function Name(params) -> ret` signature with linked and
/// highlighted types.
pub fn render_function_signature_html(
    db: &DbIndex,
    typ: &LuaType,
    func_name: &str,
    is_local: bool,
    linker: &TypeLinker,
) -> String {
    let local = if is_local {
        kw("local ")
    } else {
        String::new()
    };
    let fn_kw = kw("function ");
    let name = fn_name(func_name);
    match typ {
        LuaType::Function => format!("{local}{fn_kw}{name}()"),
        LuaType::DocFunction(func) => {
            let params: Vec<String> = func
                .get_params()
                .iter()
                .map(|(pname, ty)| match ty {
                    Some(ty) => format!(
                        "{}: {}",
                        param_name(pname),
                        render_type_html(db, ty, linker)
                    ),
                    None => param_name(pname),
                })
                .collect();
            let ret = render_type_html(db, func.get_ret(), linker);
            let ret_suffix = if ret.is_empty() {
                String::new()
            } else {
                format!(" <span class=\"hl-op\">-&gt;</span> {ret}")
            };
            format!("{local}{fn_kw}{name}({}){ret_suffix}", params.join(", "))
        }
        LuaType::Signature(signature_id) => {
            render_signature_signature_html(db, *signature_id, func_name, &local, &fn_kw, linker)
                .unwrap_or_else(|| format!("{local}{fn_kw}{name}()"))
        }
        _ => format!("{local}{fn_kw}{name}()"),
    }
}

fn render_signature_signature_html(
    db: &DbIndex,
    signature_id: LuaSignatureId,
    func_name: &str,
    local: &str,
    fn_kw: &str,
    linker: &TypeLinker,
) -> Option<String> {
    let signature = db.get_signature_index().get(&signature_id)?;
    let async_prev = match signature.async_state {
        emmylua_code_analysis::AsyncState::Async => kw("async "),
        emmylua_code_analysis::AsyncState::Sync => kw("sync "),
        _ => String::new(),
    };
    let params: Vec<String> = signature
        .get_type_params()
        .iter()
        .map(|(pname, ty)| match ty {
            Some(ty) => format!(
                "{}: {}",
                param_name(pname),
                render_type_html(db, ty, linker)
            ),
            None => param_name(pname),
        })
        .collect();
    let generics = render_generics_html(&signature.generic_params);
    let mut result = format!(
        "{async_prev}{local}{fn_kw}{name}{generics}({params})",
        name = fn_name(func_name),
        params = params.join(", ")
    );
    let rets = &signature.return_docs;
    match rets.len() {
        0 => {}
        1 => {
            let type_text = render_type_html(db, &rets[0].type_ref, linker);
            let name = rets[0].name.clone().unwrap_or_default();
            if name.is_empty() {
                result.push_str(&format!(" <span class=\"hl-op\">-&gt;</span> {type_text}"));
            } else {
                result.push_str(&format!(
                    " <span class=\"hl-op\">-&gt;</span> {} {type_text}",
                    html_escape(&name)
                ));
            }
        }
        _ => {
            let parts: Vec<String> = rets
                .iter()
                .map(|ret| {
                    let type_text = render_type_html(db, &ret.type_ref, linker);
                    let name = ret.name.clone().unwrap_or_default();
                    if name.is_empty() {
                        type_text
                    } else {
                        format!("{} {type_text}", html_escape(&name))
                    }
                })
                .collect();
            result.push_str(&format!(
                " <span class=\"hl-op\">-&gt;</span> ({})",
                parts.join(", ")
            ));
        }
    }
    Some(result)
}

fn render_function_html(
    db: &DbIndex,
    params: &[(String, Option<LuaType>)],
    ret: Option<&LuaType>,
    linker: &TypeLinker,
) -> String {
    let param_strs: Vec<String> = params
        .iter()
        .map(|(name, ty)| match ty {
            Some(ty) => format!("{}: {}", param_name(name), render_type_html(db, ty, linker)),
            None => param_name(name),
        })
        .collect();
    let ret_suffix = ret
        .map(|r| {
            let s = render_type_html(db, r, linker);
            if s.is_empty() {
                String::new()
            } else {
                format!(" <span class=\"hl-op\">-&gt;</span> {s}")
            }
        })
        .unwrap_or_default();
    format!(
        "<span class=\"hl-kw\">fun</span>({}){ret_suffix}",
        param_strs.join(", ")
    )
}

fn render_signature_html(
    db: &DbIndex,
    signature_id: LuaSignatureId,
    linker: &TypeLinker,
) -> String {
    let Some(signature) = db.get_signature_index().get(&signature_id) else {
        return "function".to_string();
    };
    let async_prev = match signature.async_state {
        emmylua_code_analysis::AsyncState::Async => kw("async "),
        emmylua_code_analysis::AsyncState::Sync => kw("sync "),
        _ => String::new(),
    };
    let param_strs: Vec<String> = signature
        .get_type_params()
        .iter()
        .map(|(name, ty)| match ty {
            Some(ty) => format!("{}: {}", param_name(name), render_type_html(db, ty, linker)),
            None => param_name(name),
        })
        .collect();
    let generics = render_generics_html(&signature.generic_params);
    let mut result = format!(
        "{async_prev}<span class=\"hl-kw\">fun</span>{generics}({})",
        param_strs.join(", ")
    );
    let rets = &signature.return_docs;
    match rets.len() {
        0 => {}
        1 => {
            let type_text = render_type_html(db, &rets[0].type_ref, linker);
            let name = rets[0].name.clone().unwrap_or_default();
            if name.is_empty() {
                result.push_str(&format!(" <span class=\"hl-op\">-&gt;</span> {type_text}"));
            } else {
                result.push_str(&format!(
                    " <span class=\"hl-op\">-&gt;</span> {} {type_text}",
                    html_escape(&name)
                ));
            }
        }
        _ => {
            let parts: Vec<String> = rets
                .iter()
                .map(|ret| {
                    let type_text = render_type_html(db, &ret.type_ref, linker);
                    let name = ret.name.clone().unwrap_or_default();
                    if name.is_empty() {
                        type_text
                    } else {
                        format!("{} {type_text}", html_escape(&name))
                    }
                })
                .collect();
            result.push_str(&format!(
                " <span class=\"hl-op\">-&gt;</span> ({})",
                parts.join(", ")
            ));
        }
    }
    result
}

fn fallback(db: &DbIndex, typ: &LuaType, linker: &TypeLinker) -> String {
    let _ = linker;
    // Documentation level does not truncate union items or member counts, so
    // large documentation types render in full.
    html_escape(&humanize_type(db, typ, RenderLevel::Documentation))
}

/// Renders the generic parameter list as `&lt;T, U&gt;`, or empty when there
/// are none.
fn render_generics_html(generic_params: &[GenericParam]) -> String {
    if generic_params.is_empty() {
        return String::new();
    }
    let names: Vec<String> = generic_params
        .iter()
        .map(|param| html_escape(&param.name))
        .collect();
    format!("&lt;{}&gt;", names.join(", "))
}

/// Renders `---@overload` signatures for a function type as `<pre>` blocks.
pub fn signature_overloads_html(
    db: &DbIndex,
    typ: &LuaType,
    func_name: &str,
    linker: &TypeLinker,
) -> Vec<String> {
    let LuaType::Signature(signature_id) = typ else {
        return Vec::new();
    };
    let Some(signature) = db.get_signature_index().get(signature_id) else {
        return Vec::new();
    };
    signature
        .overloads
        .iter()
        .map(|overload| {
            render_function_signature_html(
                db,
                &LuaType::DocFunction(overload.clone()),
                func_name,
                false,
                linker,
            )
        })
        .collect()
}

fn format_float(value: f64) -> String {
    let s = value.to_string();
    if s.contains('.') { s } else { format!("{s}.0") }
}
