use std::path::PathBuf;

use emmylua_code_analysis::{
    Emmyrc, LuaDocument, SemanticDatabase, SemanticModel, WorkspaceId, file_path_to_uri,
};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaCallArgList, LuaCallExpr, LuaComment, LuaDocDescription,
    LuaLiteralExpr, LuaStringToken, LuaSyntaxNode,
};
use emmylua_parser_desc::{DescItemKind, parse_ref_target};
use lsp_types::DocumentLink;

use crate::handlers::common::{comment_scope_types, resolve_doc_path_locations};
use crate::util::parse_desc;

pub fn build_links(
    db: &SemanticDatabase,
    model: &SemanticModel<'_>,
    root: &LuaSyntaxNode,
    document: &LuaDocument,
    emmyrc: &Emmyrc,
) -> Option<Vec<DocumentLink>> {
    let string_tokens = root
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .filter_map(LuaStringToken::cast);

    let mut result = vec![];
    for token in string_tokens {
        try_build_file_link(db, token, document, &mut result, emmyrc);
    }
    try_build_doc_links(model, db, root, document, emmyrc, &mut result);

    Some(result)
}

/// Adds links for Rust-like intra-doc references in comment descriptions.
fn try_build_doc_links(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    root: &LuaSyntaxNode,
    document: &LuaDocument,
    emmyrc: &Emmyrc,
    result: &mut Vec<DocumentLink>,
) {
    let text = document.get_text();
    let workspace_id = db
        .workspace_id_of(model.file_id())
        .unwrap_or(WorkspaceId::Main);

    for description in root.descendants().filter_map(LuaDocDescription::cast) {
        let Some(comment) = description.get_parent::<LuaComment>() else {
            continue;
        };
        let scope_types = comment_scope_types(model, &comment);
        let items = parse_desc(workspace_id, emmyrc, text, description.clone(), None);

        for item in items {
            if item.kind != DescItemKind::Ref {
                continue;
            }
            let Some(path) = parse_ref_target(text, item.range, item.range.end()) else {
                continue;
            };
            let Some(names) = path
                .iter()
                .map(|(path_item, _)| path_item.get_name().map(str::to_string))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            if names.is_empty() {
                continue;
            }
            let Some(locations) = resolve_doc_path_locations(
                model,
                db,
                &names,
                names.len().saturating_sub(1),
                &scope_types,
            ) else {
                continue;
            };
            let Some(location) = locations.first() else {
                continue;
            };
            let Some(range) = document.to_lsp_range(item.range) else {
                continue;
            };

            result.push(DocumentLink {
                target: Some(location.uri.clone()),
                range,
                tooltip: Some(names.join(".")),
                data: None,
            });
        }
    }
}

fn try_build_file_link(
    db: &SemanticDatabase,
    token: LuaStringToken,
    document: &LuaDocument,
    result: &mut Vec<DocumentLink>,
    emmyrc: &Emmyrc,
) -> Option<()> {
    if is_require_path(token.clone()).unwrap_or(false) {
        try_build_module_link(db, token, document, result);
        return Some(());
    }

    let file_path = token.get_value();
    if file_path.find(['\\', '/']).is_some() {
        let suffix_path = PathBuf::from(file_path);
        if suffix_path.exists() {
            if let Some(uri) = file_path_to_uri(&suffix_path) {
                let document_link = DocumentLink {
                    target: Some(uri),
                    range: document.to_lsp_range(token.get_range())?,
                    tooltip: None,
                    data: None,
                };

                result.push(document_link);
            }
            return Some(());
        }

        let resource_paths = emmyrc.resource.paths.clone();
        for resource_path in resource_paths {
            let full_path = PathBuf::from(resource_path).join(&suffix_path);
            if full_path.exists() {
                if let Some(uri) = file_path_to_uri(&full_path) {
                    let document_link = DocumentLink {
                        target: Some(uri),
                        range: document.to_lsp_range(token.get_range())?,
                        tooltip: None,
                        data: None,
                    };

                    result.push(document_link);
                }
                return Some(());
            }
        }
    }

    Some(())
}

fn try_build_module_link(
    db: &SemanticDatabase,
    token: LuaStringToken,
    document: &LuaDocument,
    result: &mut Vec<DocumentLink>,
) -> Option<()> {
    let module_path = token.get_value();
    let file_id = db.module_file_from(&module_path, document.get_file_id())?;
    let uri = db.file_uri(file_id)?;
    let range = token.get_range();
    let lsp_range = document.to_lsp_range(range)?;
    let document_link = DocumentLink {
        target: Some(uri.clone()),
        range: lsp_range,
        tooltip: None,
        data: None,
    };

    result.push(document_link);

    Some(())
}

pub fn is_require_path(token: LuaStringToken) -> Option<bool> {
    let call_expr = token
        .get_parent::<LuaLiteralExpr>()?
        .get_parent::<LuaCallArgList>()?
        .get_parent::<LuaCallExpr>()?;

    Some(call_expr.is_require())
}

#[cfg(test)]
mod tests {
    use super::build_links;
    use crate::handlers::test_lib::ProviderVirtualWorkspace;
    use emmylua_parser::LuaAstNode;
    use lsp_types::DocumentLink;

    fn build_doc_links(ws: &mut ProviderVirtualWorkspace, source: &str) -> Vec<DocumentLink> {
        let file_id = ws.def(source);
        let analysis = &ws.analysis;
        let model = analysis.semantic_model(file_id);
        let root = model.chunk().unwrap();
        let document = analysis.db.document(file_id).unwrap();
        let emmyrc = analysis.get_emmyrc();
        build_links(&analysis.db, &model, root.syntax(), &document, &emmyrc).unwrap()
    }

    fn define_object_kinds(ws: &mut ProviderVirtualWorkspace) {
        ws.def_file(
            "types.lua",
            r#"
---@class Widget
---@field name string

---@class ns.Shape
---@field area number

---@enum Color
local Color = {
    Red = 1,
}

---@alias Handler fun(value: string): boolean
"#,
        );
        ws.def_file(
            "fio.lua",
            r#"
local fio = {}
function fio.open() end
return fio
"#,
        );
        ws.def_file(
            "globals.lua",
            r#"
g_config = {}
function g_config.get() end
"#,
        );
    }

    #[test]
    fn builds_intra_doc_symbol_links_for_all_kinds() {
        let mut ws = ProviderVirtualWorkspace::new();
        define_object_kinds(&mut ws);
        let links = build_doc_links(
            &mut ws,
            r#"
--- See [`Widget`], [`ns.Shape`], [`Color`], [`Color.Red`], [`Handler`], [`fio`], [`fio.open`], [`g_config`], [`g_config.get`].
local x = 1
"#,
        );

        let mut actual: Vec<(String, String)> = links
            .iter()
            .filter_map(|link| {
                let tooltip = link.tooltip.clone()?;
                let target = link.target.as_ref()?.as_str().to_string();
                Some((tooltip, target))
            })
            .collect();
        actual.sort();

        let expected = [
            ("Color", "types.lua"),
            ("Color.Red", "types.lua"),
            ("Handler", "types.lua"),
            ("Widget", "types.lua"),
            ("fio", "fio.lua"),
            ("fio.open", "fio.lua"),
            ("g_config", "globals.lua"),
            ("g_config.get", "globals.lua"),
            ("ns.Shape", "types.lua"),
        ];
        assert_eq!(actual.len(), expected.len(), "links: {actual:?}");
        for (tooltip, file) in expected {
            let target = actual
                .iter()
                .find(|(actual_tooltip, _)| actual_tooltip == tooltip)
                .unwrap_or_else(|| panic!("missing link for {tooltip}: {actual:?}"));
            assert!(
                target.1.contains(file),
                "unexpected target for {tooltip}: {}",
                target.1
            );
        }
    }

    #[test]
    fn builds_link_in_param_description() {
        let mut ws = ProviderVirtualWorkspace::new();
        define_object_kinds(&mut ws);
        let links = build_doc_links(
            &mut ws,
            r#"
---@param value string See [`Widget`].
local function f(value) end
"#,
        );

        assert_eq!(links.len(), 1, "links: {links:?}");
        assert_eq!(links[0].tooltip.as_deref(), Some("Widget"));
    }

    #[test]
    fn ignores_non_link_forms() {
        let mut ws = ProviderVirtualWorkspace::new();
        define_object_kinds(&mut ws);
        let links = build_doc_links(
            &mut ws,
            r#"
--- See [`Missing`], [Widget] and [`Widget`](https://example.com).
local x = 1
"#,
        );

        assert!(
            links.is_empty(),
            "plain/explicit/unresolved links must not produce document links: {links:?}"
        );
    }
}
