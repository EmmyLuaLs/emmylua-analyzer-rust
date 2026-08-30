use emmylua_code_analysis::{DocSyntax, Emmyrc, WorkspaceId};
use emmylua_parser::LuaDocDescription;
use emmylua_parser_desc::{DescItem, DescParserType};

/// Parse the doc comment description into a list of description items (used for text-only features such as documentation selection range).
///
/// Doc reference resolution has moved to the salsa path (see `handlers/common/salsa_reference.rs`),
/// so the old DbIndex `resolve_ref` family is no longer retained here.
pub fn parse_desc(
    workspace_id: WorkspaceId,
    emmyrc: &Emmyrc,
    text: &str,
    desc: LuaDocDescription,
    offset: Option<usize>,
) -> Vec<DescItem> {
    let parser_kind = if workspace_id == WorkspaceId::STD {
        DescParserType::Md
    } else {
        match emmyrc.doc.syntax {
            DocSyntax::None => DescParserType::None,
            DocSyntax::Md => DescParserType::Md,
            DocSyntax::Myst => DescParserType::MySt {
                primary_domain: emmyrc.doc.rst_primary_domain.clone(),
            },
            DocSyntax::Rst => DescParserType::Rst {
                primary_domain: emmyrc.doc.rst_primary_domain.clone(),
                default_role: emmyrc.doc.rst_default_role.clone(),
            },
        }
    };

    emmylua_parser_desc::parse(parser_kind, text, desc, offset)
}
