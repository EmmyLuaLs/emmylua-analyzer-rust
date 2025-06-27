use emmylua_parser::{
    LuaAst, LuaAstNode, LuaClosureExpr, LuaDocTag, LuaExpr, LuaLocalName, LuaVarExpr,
};

use crate::{
    db_index::{LuaMemberId, LuaSemanticDeclId, LuaSignatureId},
    LuaDeclId,
};

use super::{
    diagnostic_tags::analyze_diagnostic,
    field_or_operator_def_tags::{analyze_field, analyze_operator},
    property_tags::{
        analyze_async, analyze_deprecated, analyze_export, analyze_nodiscard, analyze_source,
        analyze_version, analyze_visibility,
    },
    type_def_tags::{analyze_alias, analyze_class, analyze_enum, analyze_func_generic},
    type_ref_tags::{
        analyze_as, analyze_cast, analyze_module, analyze_other, analyze_overload, analyze_param,
        analyze_return, analyze_return_cast, analyze_see, analyze_type,
    },
    DocAnalyzer,
};

pub fn analyze_tag(analyzer: &mut DocAnalyzer, tag: LuaDocTag) -> Option<()> {
    match tag {
        // def
        LuaDocTag::Class(class) => {
            analyze_class(analyzer, class)?;
        }
        LuaDocTag::Generic(generic) => {
            analyze_func_generic(analyzer, generic)?;
        }
        LuaDocTag::Enum(enum_tag) => {
            analyze_enum(analyzer, enum_tag)?;
        }
        LuaDocTag::Alias(alias) => {
            analyze_alias(analyzer, alias)?;
        }

        // ref
        LuaDocTag::Type(type_tag) => {
            analyze_type(analyzer, type_tag)?;
        }
        LuaDocTag::Param(param_tag) => {
            analyze_param(analyzer, param_tag)?;
        }
        LuaDocTag::Return(return_tag) => {
            analyze_return(analyzer, return_tag)?;
        }
        LuaDocTag::ReturnCast(return_cast) => {
            analyze_return_cast(analyzer, return_cast)?;
        }
        LuaDocTag::Overload(overload_tag) => {
            analyze_overload(analyzer, overload_tag)?;
        }
        LuaDocTag::Module(module_tag) => {
            analyze_module(analyzer, module_tag)?;
        }

        // property
        LuaDocTag::Visibility(kind) => {
            analyze_visibility(analyzer, kind)?;
        }
        LuaDocTag::Source(source) => {
            analyze_source(analyzer, source)?;
        }
        LuaDocTag::Nodiscard(nodiscard) => {
            analyze_nodiscard(analyzer, nodiscard)?;
        }
        LuaDocTag::Deprecated(deprecated) => {
            analyze_deprecated(analyzer, deprecated)?;
        }
        LuaDocTag::Version(version) => {
            analyze_version(analyzer, version)?;
        }
        LuaDocTag::Async(_) => {
            analyze_async(analyzer)?;
        }

        // field or operator
        LuaDocTag::Field(filed) => {
            analyze_field(analyzer, filed)?;
        }
        LuaDocTag::Operator(operator) => {
            analyze_operator(analyzer, operator)?;
        }

        // diagnostic
        LuaDocTag::Diagnostic(diagnostic) => {
            analyze_diagnostic(analyzer, diagnostic)?;
        }
        // as type
        LuaDocTag::As(lua_doc_tag_as) => {
            analyze_as(analyzer, lua_doc_tag_as)?;
        }
        // cast type
        LuaDocTag::Cast(lua_doc_tag_cast) => {
            analyze_cast(analyzer, lua_doc_tag_cast)?;
        }
        LuaDocTag::See(see) => {
            analyze_see(analyzer, see)?;
        }
        LuaDocTag::Other(other) => {
            analyze_other(analyzer, other)?;
        }
        LuaDocTag::Export(export) => {
            analyze_export(analyzer, export)?;
        }
        _ => {}
    }

    Some(())
}

pub fn find_owner_closure(analyzer: &DocAnalyzer) -> Option<LuaClosureExpr> {
    if let Some(owner) = analyzer.comment.get_owner() {
        match owner {
            LuaAst::LuaFuncStat(func) => {
                if let Some(closure) = func.get_closure() {
                    return Some(closure);
                }
            }
            LuaAst::LuaLocalFuncStat(local_func) => {
                if let Some(closure) = local_func.get_closure() {
                    return Some(closure);
                }
            }
            owner => {
                return owner.descendants::<LuaClosureExpr>().next();
            }
        }
    }

    None
}

pub fn get_owner_id(analyzer: &mut DocAnalyzer) -> Option<LuaSemanticDeclId> {
    let owner = analyzer.comment.get_owner()?;
    match owner {
        LuaAst::LuaAssignStat(assign) => {
            let first_var = assign.child::<LuaVarExpr>()?;
            match first_var {
                LuaVarExpr::NameExpr(name_expr) => {
                    let decl_id = LuaDeclId::new(analyzer.file_id, name_expr.get_position());
                    let _ = analyzer.db.get_decl_index_mut().get_decl_mut(&decl_id)?;
                    return Some(LuaSemanticDeclId::LuaDecl(decl_id));
                }
                LuaVarExpr::IndexExpr(index_expr) => {
                    let member_id = LuaMemberId::new(index_expr.get_syntax_id(), analyzer.file_id);
                    return Some(LuaSemanticDeclId::Member(member_id));
                } // _ => None,
            }
        }
        LuaAst::LuaLocalStat(local_stat) => {
            let local_name = local_stat.child::<LuaLocalName>()?;
            let decl_id = LuaDeclId::new(analyzer.file_id, local_name.get_position());
            return Some(LuaSemanticDeclId::LuaDecl(decl_id));
        }
        LuaAst::LuaTableField(field) => {
            let member_id = LuaMemberId::new(field.get_syntax_id(), analyzer.file_id);
            return Some(LuaSemanticDeclId::Member(member_id));
        }
        LuaAst::LuaCallExprStat(call_expr_stat) => {
            let call_expr = call_expr_stat.get_call_expr()?;
            let call_args = call_expr.get_args_list()?;
            for call_arg in call_args.get_args() {
                if let LuaExpr::ClosureExpr(closure) = call_arg {
                    return Some(LuaSemanticDeclId::Signature(LuaSignatureId::from_closure(
                        analyzer.file_id,
                        &closure,
                    )));
                }
            }
            None
        }
        LuaAst::LuaClosureExpr(closure) => Some(LuaSemanticDeclId::Signature(
            LuaSignatureId::from_closure(analyzer.file_id, &closure),
        )),
        _ => {
            let closure = find_owner_closure(analyzer)?;
            Some(LuaSemanticDeclId::Signature(LuaSignatureId::from_closure(
                analyzer.file_id,
                &closure,
            )))
        }
    }
}
