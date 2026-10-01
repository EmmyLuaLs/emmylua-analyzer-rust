use super::{
    desc::{member_description, preceding_field_description},
    render::humanize,
};
use emmylua_code_analysis::{SemanticDatabase, SemanticModel};
use emmylua_parser::{LuaAstNode, LuaDocTagField, LuaSyntaxToken};
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

pub(super) fn hover(
    model: &SemanticModel,
    db: &SemanticDatabase,
    token: &LuaSyntaxToken,
) -> Option<Hover> {
    let pair = super::super::common::directional_index_at(model, token)
        .map(|(read, write, _)| (read, write))
        .or_else(|| {
            let field = token.parent()?.ancestors().find_map(LuaDocTagField::cast)?;
            if !field
                .get_field_key_range()?
                .contains_range(token.text_range())
            {
                return None;
            }
            model.directional_declaration(&model.find_decl(token.clone().into())?)
        })?;
    let (read, write) = pair;
    let mut value = format!(
        "```lua\nRead: {}\nWrite: {}\n```",
        humanize(model, &read.typ),
        humanize(model, &write.typ)
    );
    if let Some(id) = read.id.as_ref() {
        let desc = member_description(model, id);
        let text = desc.text.or_else(|| preceding_field_description(model, id));
        if let Some(text) = text {
            value.push_str("\n\n");
            value.push_str(&text);
        }
    }
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: db
            .document(model.file_id())?
            .to_lsp_range(token.text_range()),
    })
}
