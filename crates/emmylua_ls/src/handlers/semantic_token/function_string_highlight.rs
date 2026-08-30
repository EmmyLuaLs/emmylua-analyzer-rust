use emmylua_code_analysis::{LuaFunctionType, LuaType, SalsaSemanticModel};
use emmylua_parser::{LuaAstNode, LuaAstToken, LuaCallExpr, LuaStringToken};
use emmylua_parser_desc::CodeBlockLang;

use crate::handlers::semantic_token::{
    language_injector::process_inject_lang_string_token, semantic_token_builder::SemanticBuilder,
};

pub fn fun_string_highlight(
    builder: &mut SemanticBuilder,
    model: &SalsaSemanticModel<'_>,
    call_expr: LuaCallExpr,
    string_token: &LuaStringToken,
) -> Option<()> {
    let prefix = call_expr.get_prefix_expr()?;
    let prefix_ty = model.type_of_expr(prefix.get_syntax_id());
    let func = extract_function_type(&prefix_ty)?;
    let params = func.get_params();
    let mut param_idx = call_expr
        .get_args_list()?
        .get_args()
        .position(|arg| arg.get_position() == string_token.get_position())?;

    let colon_define = func.is_colon_define();
    let colon_call = call_expr.is_colon_call();

    match (colon_define, colon_call) {
        (true, false) => {
            param_idx = param_idx.saturating_sub(1);
        }
        (false, true) => {
            param_idx += 1;
        }
        _ => {}
    }

    let (_, opt_typ) = params.get(param_idx)?;
    let param_type = opt_typ.as_ref()?;
    let lang_name = get_lang_str_from_type(param_type)?;
    if let Some(lang) = CodeBlockLang::try_parse(&lang_name) {
        process_inject_lang_string_token(builder, lang, string_token);
    }
    Some(())
}

fn extract_function_type(ty: &LuaType) -> Option<LuaFunctionType> {
    match ty {
        LuaType::DocFunction(func) => Some(func.as_ref().clone()),
        LuaType::Union(u) => u.into_vec().iter().find_map(extract_function_type),
        _ => None,
    }
}

fn get_lang_str_from_type(typ: &LuaType) -> Option<String> {
    match typ {
        LuaType::Language(s) => return Some(s.to_string()),
        LuaType::Union(u) => {
            for sub_type in u.into_vec() {
                if let LuaType::Language(s) = sub_type {
                    return Some(s.to_string());
                }
            }
        }
        _ => {}
    }

    None
}
