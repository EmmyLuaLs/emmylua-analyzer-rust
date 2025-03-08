use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaClosureExpr, LuaExpr, LuaIndexExpr, LuaIndexKey,
    LuaLiteralExpr, LuaLiteralToken, LuaNameExpr, LuaTableExpr, LuaVarExpr,
};

use crate::{
    db_index::{LuaDecl, LuaMember, LuaMemberKey, LuaMemberOwner},
    FileId, InFiled, LuaDeclExtra, LuaDeclId, LuaMemberId, LuaSignatureId,
};

use super::DeclAnalyzer;

pub fn analyze_name_expr(analyzer: &mut DeclAnalyzer, expr: LuaNameExpr) -> Option<()> {
    let name_token = expr.get_name_token()?;
    let name = name_token.get_name_text();
    let position = name_token.get_position();
    let range = name_token.get_range();
    let file_id = analyzer.get_file_id();
    let decl_id = LuaDeclId::new(file_id, position);
    let (decl_id, is_local) = if analyzer.decl.get_decl(&decl_id).is_some() {
        (Some(decl_id), false)
    } else if let Some(decl) = analyzer.find_decl(&name, position) {
        if decl.is_local() {
            // reference local variable
            (Some(decl.get_id()), true)
        } else {
            if decl.get_position() == position {
                return Some(());
            }
            // reference in filed global variable
            (Some(decl.get_id()), false)
        }
    } else {
        (None, false)
    };

    let reference_index = analyzer.db.get_reference_index_mut();

    if let Some(id) = decl_id {
        reference_index.add_decl_reference(id, file_id, range, false);
    }

    if !is_local {
        reference_index.add_global_reference(name, file_id, expr.get_syntax_id());
    }

    Some(())
}

pub fn analyze_index_expr(analyzer: &mut DeclAnalyzer, index_expr: LuaIndexExpr) -> Option<()> {
    let index_key = index_expr.get_index_key()?;
    let key = match index_key {
        LuaIndexKey::Name(name) => LuaMemberKey::Name(name.get_name_text().to_string().into()),
        LuaIndexKey::Integer(int) => {
            if int.is_int() {
                LuaMemberKey::Integer(int.get_int_value())
            } else {
                return None;
            }
        }
        LuaIndexKey::String(string) => LuaMemberKey::Name(string.get_value().into()),
        LuaIndexKey::Expr(_) => return None,
        LuaIndexKey::Idx(i) => LuaMemberKey::Integer(i as i64),
    };

    let file_id = analyzer.get_file_id();
    let syntax_id = index_expr.get_syntax_id();
    let prefix = index_expr.get_prefix_expr()?;
    if let LuaExpr::NameExpr(name_expr) = prefix {
        let name_token = name_expr.get_name_token()?;
        let name_token_text = name_token.get_name_text();
        if name_token_text == "_G" || name_token_text == "_ENV" {
            if let LuaMemberKey::Name(name) = &key {
                analyzer
                    .db
                    .get_reference_index_mut()
                    .add_global_reference(name, file_id, syntax_id);
            }

            return Some(());
        }
    }

    analyzer
        .db
        .get_reference_index_mut()
        .add_index_reference(key, file_id, syntax_id);

    Some(())
}

pub fn analyze_closure_expr(analyzer: &mut DeclAnalyzer, expr: LuaClosureExpr) -> Option<()> {
    let params = expr.get_params_list()?;
    let signature_id = LuaSignatureId::from_closure(analyzer.get_file_id(), &expr);
    let file_id = analyzer.get_file_id();
    let member_id = get_closure_member_id(&expr, file_id);
    for (idx, param) in params.get_params().enumerate() {
        let name = param.get_name_token().map_or_else(
            || {
                if param.is_dots() {
                    "...".to_string()
                } else {
                    "".to_string()
                }
            },
            |name_token| name_token.get_name_text().to_string(),
        );
        let range = param.get_range();

        let decl = LuaDecl::new(
            &name,
            file_id,
            range,
            LuaDeclExtra::Param {
                idx,
                signature_id,
                owner_member_id: member_id,
            },
            None,
        );

        analyzer.add_decl(decl);
    }

    analyze_closure_params(analyzer, &signature_id, &expr);

    Some(())
}

fn get_closure_member_id(closure: &LuaClosureExpr, file_id: FileId) -> Option<LuaMemberId> {
    let parent = closure.get_parent::<LuaAst>()?;
    match parent {
        LuaAst::LuaAssignStat(assign) => {
            let (vars, value_exprs) = assign.get_var_and_expr_list();
            let value_idx = value_exprs
                .iter()
                .position(|expr| expr.get_position() == closure.get_position())?;
            let var = vars.get(value_idx)?;
            if let LuaVarExpr::IndexExpr(index_expr) = var {
                return Some(LuaMemberId::new(index_expr.get_syntax_id(), file_id));
            }
        }
        LuaAst::LuaFuncStat(func_stat) => {
            let func_name = func_stat.get_func_name()?;
            if let LuaVarExpr::IndexExpr(index_expr) = func_name {
                return Some(LuaMemberId::new(index_expr.get_syntax_id(), file_id));
            }
        }
        LuaAst::LuaTableField(table_field) => {
            return Some(LuaMemberId::new(table_field.get_syntax_id(), file_id));
        }
        _ => {}
    }

    None
}

fn analyze_closure_params(
    analyzer: &mut DeclAnalyzer,
    signature_id: &LuaSignatureId,
    closure: &LuaClosureExpr,
) -> Option<()> {
    let signature = analyzer
        .db
        .get_signature_index_mut()
        .get_or_create(signature_id.clone());
    let params = closure.get_params_list()?.get_params();
    for param in params {
        let name = if let Some(name_token) = param.get_name_token() {
            name_token.get_name_text().to_string()
        } else if param.is_dots() {
            "...".to_string()
        } else {
            return None;
        };

        signature.params.push(name);
    }

    Some(())
}

pub fn analyze_table_expr(analyzer: &mut DeclAnalyzer, expr: LuaTableExpr) -> Option<()> {
    if expr.is_object() {
        let file_id = analyzer.get_file_id();
        let owner_id = LuaMemberOwner::Element(InFiled {
            file_id,
            value: expr.get_range(),
        });

        for field in expr.get_fields() {
            if let Some(field_key) = field.get_field_key() {
                let key: LuaMemberKey = field_key.into();
                if key.is_none() {
                    continue;
                }

                analyzer.db.get_reference_index_mut().add_index_reference(
                    key.clone(),
                    file_id,
                    field.get_syntax_id(),
                );

                let member =
                    LuaMember::new(owner_id.clone(), key, file_id, field.get_syntax_id(), None);
                analyzer.db.get_member_index_mut().add_member(member);
            }
        }
    }

    Some(())
}

pub fn analyze_literal_expr(analyzer: &mut DeclAnalyzer, expr: LuaLiteralExpr) -> Option<()> {
    let literal = expr.get_literal()?;
    let file_id = analyzer.get_file_id();

    match literal {
        LuaLiteralToken::String(string_token) => {
            let value = string_token.get_value();
            if value.len() <= 64 {
                analyzer.db.get_reference_index_mut().add_string_reference(
                    file_id,
                    &value,
                    string_token.get_range(),
                );
            }
        }
        LuaLiteralToken::Dots(dots_token) => {
            let position = dots_token.get_position();
            let range = dots_token.get_range();

            let decl_id = LuaDeclId::new(file_id, position);
            let decl_id = analyzer
                .decl
                .get_decl(&decl_id)
                .map(|_| decl_id)
                .or_else(|| {
                    analyzer
                        .find_decl(&dots_token.get_text(), position)
                        .and_then(|decl| decl.is_local().then(|| decl.get_id()))
                });

            if let Some(id) = decl_id {
                analyzer
                    .db
                    .get_reference_index_mut()
                    .add_decl_reference(id, file_id, range, false);
            }
        }
        _ => {}
    }

    Some(())
}
