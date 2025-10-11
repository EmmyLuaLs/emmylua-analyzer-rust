use emmylua_parser::{
    LuaAst, LuaAstNode, LuaDocTagAttributeUse, LuaDocType, LuaExpr, LuaKind, LuaLiteralExpr,
    LuaLiteralToken, LuaSyntaxKind, LuaSyntaxNode,
};
use smol_str::SmolStr;

use crate::{
    LuaAttributeUse, LuaType,
    compilation::analyzer::doc::{
        DocAnalyzer,
        infer_type::infer_type,
        tags::{get_owner_id, report_orphan_tag},
    },
};

pub fn analyze_tag_attribute_use(
    analyzer: &mut DocAnalyzer,
    tag_use: LuaDocTagAttributeUse,
) -> Option<()> {
    let owner = attribute_use_get_owner(analyzer, &tag_use);
    let owner_id = match get_owner_id(analyzer, owner, true) {
        Some(id) => id,
        None => {
            report_orphan_tag(analyzer, &tag_use);
            return None;
        }
    };
    let attribute_uses = infer_attribute_uses(analyzer, tag_use)?;
    for attribute_use in attribute_uses {
        analyzer.db.get_property_index_mut().add_attribute_use(
            analyzer.file_id,
            owner_id.clone(),
            attribute_use,
        );
    }
    Some(())
}

pub fn infer_attribute_uses(
    analyzer: &mut DocAnalyzer,
    tag_use: LuaDocTagAttributeUse,
) -> Option<Vec<LuaAttributeUse>> {
    let attribute_uses = tag_use.get_attribute_uses();
    let mut result = Vec::new();
    for attribute_use in attribute_uses {
        let mut params = Vec::new();
        if let Some(attribute_call_arg_list) = attribute_use.get_arg_list() {
            for arg in attribute_call_arg_list.get_args() {
                let arg_type = infer_attribute_arg_type(arg);
                params.push(arg_type);
            }
        }
        let attribute_type = infer_type(analyzer, LuaDocType::Name(attribute_use.get_type()?));
        if let LuaType::Ref(type_id) = attribute_type {
            result.push(LuaAttributeUse::new(type_id, params));
        }
    }
    Some(result)
}

fn infer_attribute_arg_type(expr: LuaLiteralExpr) -> LuaType {
    if let Some(literal_token) = expr.get_literal() {
        match literal_token {
            LuaLiteralToken::String(str_token) => {
                return LuaType::DocStringConst(SmolStr::new(str_token.get_value()).into());
            }
            LuaLiteralToken::Number(number_token) => {
                if number_token.is_int() {
                    return LuaType::DocIntegerConst(number_token.get_int_value());
                } else {
                    return LuaType::Number;
                }
            }
            LuaLiteralToken::Bool(bool_token) => {
                return LuaType::DocBooleanConst(bool_token.is_true());
            }
            LuaLiteralToken::Nil(_) => return LuaType::Nil,
            // todo
            LuaLiteralToken::Dots(_) => return LuaType::Any,
            LuaLiteralToken::Question(_) => return LuaType::Nil,
        }
    }
    LuaType::Unknown
}

/// 特性的寻找所有者需要特殊处理
fn attribute_use_get_owner(
    analyzer: &mut DocAnalyzer,
    attribute_use: &LuaDocTagAttributeUse,
) -> Option<LuaAst> {
    // 针对 ---@field 特殊处理
    if let Some(attached_node) = attribute_find_doc_field(&attribute_use.syntax()) {
        return LuaAst::cast(attached_node);
    }
    // 回退
    analyzer.comment.get_owner()
}

fn attribute_find_doc_field(comment: &LuaSyntaxNode) -> Option<LuaSyntaxNode> {
    let mut next_sibling = comment.next_sibling();
    loop {
        next_sibling.as_ref()?;
        if let Some(sibling) = &next_sibling {
            match sibling.kind() {
                LuaKind::Syntax(LuaSyntaxKind::DocTagField) => {
                    return Some(sibling.clone());
                }
                LuaKind::Syntax(LuaSyntaxKind::Comment) => {
                    return None;
                }
                LuaKind::Syntax(LuaSyntaxKind::Block) => {
                    return None;
                }
                _ => {
                    if LuaExpr::can_cast(sibling.kind().into()) {
                        return None;
                    }
                }
            }
            next_sibling = sibling.next_sibling();
        }
    }
}
