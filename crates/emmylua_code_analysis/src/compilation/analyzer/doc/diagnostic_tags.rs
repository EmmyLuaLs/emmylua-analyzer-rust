use emmylua_parser::{LuaAstToken, LuaDocTagDiagnostic};

use super::DocAnalyzer;

pub fn analyze_diagnostic(
    _analyzer: &mut DocAnalyzer,
    diagnostic: LuaDocTagDiagnostic,
) -> Option<()> {
    let token = diagnostic.get_action_token()?;
    let action = token.get_text();
    match action {
        "disable" | "disable-next-line" | "disable-line" | "enable" => {}
        _ => {}
    };

    Some(())
}
