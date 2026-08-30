use emmylua_code_analysis::{LuaType, LuaTypeDeclId, RenderLevel};

use crate::doc_model::{DocModel, DocTypeKey};

use super::render::html_escape;

/// A callback that maps a type declaration id to a page href, or `None` when
/// the type is not documented.
pub type TypeLinker<'a> = dyn Fn(&DocTypeKey) -> Option<String> + 'a;

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
pub fn render_type_html(model: &DocModel, typ: &LuaType, linker: &TypeLinker) -> String {
    render_type_style(model, typ, linker, TypeStyle::Inline)
}

/// Recursively renders a `LuaType` as HTML with the given layout style.
pub fn render_type_style(
    model: &DocModel,
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
        LuaType::Ref(id) | LuaType::Def(id) => render_named(model, id, linker),

        // ─── composite ──────────────────────────────────────────────
        LuaType::Array(arr) => format!(
            "{}[]",
            render_type_style(model, arr.get_base(), linker, TypeStyle::Inline)
        ),
        LuaType::Tuple(tuple) => {
            let parts: Vec<String> = tuple
                .get_types()
                .iter()
                .map(|t| render_type_style(model, t, linker, TypeStyle::Inline))
                .collect();
            format!("({})", parts.join(", "))
        }
        LuaType::Union(union) => {
            let parts: Vec<String> = union
                .into_vec()
                .iter()
                .map(|t| render_type_style(model, t, linker, TypeStyle::Inline))
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
                    let body = render_type_style(model, t, linker, TypeStyle::Inline);
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
                .map(|t| render_type_style(model, t, linker, TypeStyle::Inline))
                .collect();
            parts.join(" & ")
        }
        LuaType::DocFunction(func) => {
            render_function_html(model, func.get_params(), Some(func.get_ret()), linker)
        }
        LuaType::Signature(_) => type_kw("fun"),
        LuaType::Generic(generic) => {
            let base = render_named(model, &generic.get_base_type_id(), linker);
            let params: Vec<String> = generic
                .get_params()
                .iter()
                .map(|t| render_type_style(model, t, linker, TypeStyle::Inline))
                .collect();
            format!("{}&lt;{}&gt;", base, params.join(", "))
        }
        LuaType::TableGeneric(params) => {
            let parts: Vec<String> = params
                .iter()
                .map(|t| render_type_style(model, t, linker, TypeStyle::Inline))
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
                render_type_style(model, inner, linker, TypeStyle::Inline)
            ),
            None => "...".to_string(),
        },
        LuaType::Instance(instance) => {
            render_type_style(model, instance.get_base(), linker, TypeStyle::Inline)
        }
        LuaType::Namespace(ns) => format!("{{ {} }}", html_escape(ns)),
        LuaType::ModuleRef(file_id) => model
            .module_name(*file_id)
            .map(html_escape)
            .unwrap_or_else(|| "module".to_string()),

        // ─── complex / exotic → salsa text-rendering fallback ───────────
        LuaType::TableConst(_)
        | LuaType::Object(_)
        | LuaType::StrTplRef(_)
        | LuaType::Call(_)
        | LuaType::TypeGuard(_)
        | LuaType::Conditional(_)
        | LuaType::Mapped(_) => fallback(model, typ),
    }
}

fn render_named(model: &DocModel, id: &LuaTypeDeclId, linker: &TypeLinker) -> String {
    let key = DocTypeKey::from_lua_id(id);
    let name = model.type_name(id);
    if let Some(href) = linker(&key) {
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
pub fn render_const_type_html(model: &DocModel, typ: &LuaType, linker: &TypeLinker) -> String {
    let value = render_type_html(model, typ, linker);
    match typ {
        LuaType::IntegerConst(_) | LuaType::DocIntegerConst(_) => format!("integer = {value}"),
        LuaType::FloatConst(_) => format!("number = {value}"),
        LuaType::StringConst(_) | LuaType::DocStringConst(_) => format!("string = {value}"),
        _ => value,
    }
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
    model: &DocModel,
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
                        render_type_html(model, ty, linker)
                    ),
                    None => param_name(pname),
                })
                .collect();
            let ret = render_type_html(model, func.get_ret(), linker);
            let ret_suffix = if ret.is_empty() {
                String::new()
            } else {
                format!(" <span class=\"hl-op\">-&gt;</span> {ret}")
            };
            format!("{local}{fn_kw}{name}({}){ret_suffix}", params.join(", "))
        }
        _ => format!("{local}{fn_kw}{name}()"),
    }
}

fn render_function_html(
    model: &DocModel,
    params: &[(String, Option<LuaType>)],
    ret: Option<&LuaType>,
    linker: &TypeLinker,
) -> String {
    let param_strs: Vec<String> = params
        .iter()
        .map(|(name, ty)| match ty {
            Some(ty) => format!(
                "{}: {}",
                param_name(name),
                render_type_html(model, ty, linker)
            ),
            None => param_name(name),
        })
        .collect();
    let ret_suffix = ret
        .map(|r| {
            let s = render_type_html(model, r, linker);
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

fn fallback(model: &DocModel, typ: &LuaType) -> String {
    html_escape(&model.render_type(typ, RenderLevel::Documentation))
}

/// Renders `---@overload` signatures for a function type as `<pre>` blocks.
pub fn signature_overloads_html(
    model: &DocModel,
    overloads: &[LuaType],
    func_name: &str,
    linker: &TypeLinker,
) -> Vec<String> {
    overloads
        .iter()
        .map(|overload| render_function_signature_html(model, overload, func_name, false, linker))
        .collect()
}

fn format_float(value: f64) -> String {
    let s = value.to_string();
    if s.contains('.') { s } else { format!("{s}.0") }
}
