use emmylua_parser::{
    LuaAstNode, LuaCallExpr, LuaChunk, LuaClosureExpr, LuaExpr, LuaFuncStat, LuaLocalFuncStat,
    LuaVarExpr,
};
use rowan::TextSize;
use smol_str::SmolStr;

use super::super::{
    SalsaCallKindSummary, SalsaCallSummary, SalsaDocOwnerKindSummary, SalsaDocSummary,
    SalsaDocTypeNodeKey, SalsaSignatureIndexSummary, SalsaSignatureParamSummary,
    SalsaSignatureSourceSummary, SalsaSignatureSummary,
};
use super::analyze_signature_return_points;

pub fn analyze_signature_summary(
    chunk: LuaChunk,
    doc: &SalsaDocSummary,
) -> SalsaSignatureIndexSummary {
    SalsaSignatureAnalyzer::new(chunk, doc).analyze()
}

struct SalsaSignatureAnalyzer<'a> {
    chunk: LuaChunk,
    doc: &'a SalsaDocSummary,
}

impl<'a> SalsaSignatureAnalyzer<'a> {
    fn new(chunk: LuaChunk, doc: &'a SalsaDocSummary) -> Self {
        Self { chunk, doc }
    }

    fn analyze(&self) -> SalsaSignatureIndexSummary {
        let signatures = self
            .chunk
            .descendants::<LuaClosureExpr>()
            .filter_map(|closure| self.build_signature_summary(closure))
            .collect();
        let calls = self
            .chunk
            .descendants::<LuaCallExpr>()
            .map(build_call_summary)
            .collect();

        SalsaSignatureIndexSummary { signatures, calls }
    }

    fn build_signature_summary(&self, closure: LuaClosureExpr) -> Option<SalsaSignatureSummary> {
        let signature_offset = closure.get_position();
        let (owner_offset, owner_kind, source, name, is_method) =
            if let Some(func_stat) = closure.get_parent::<LuaFuncStat>() {
                (
                    func_stat.get_position(),
                    SalsaDocOwnerKindSummary::FuncStat,
                    SalsaSignatureSourceSummary::FuncStat,
                    extract_function_name(func_stat.get_func_name()),
                    is_method_func_stat(&func_stat),
                )
            } else if let Some(local_func_stat) = closure.get_parent::<LuaLocalFuncStat>() {
                (
                    local_func_stat.get_position(),
                    SalsaDocOwnerKindSummary::LocalFuncStat,
                    SalsaSignatureSourceSummary::LocalFuncStat,
                    local_func_stat
                        .get_local_name()
                        .and_then(|local_name| local_name.get_name_token())
                        .map(|name_token| name_token.get_name_text().into()),
                    false,
                )
            } else {
                (
                    signature_offset,
                    SalsaDocOwnerKindSummary::Closure,
                    SalsaSignatureSourceSummary::ClosureExpr,
                    None,
                    false,
                )
            };

        let params = closure
            .get_params_list()
            .into_iter()
            .flat_map(|param_list| param_list.get_params())
            .filter_map(|param| {
                let name = param.get_name_token().map_or_else(
                    || {
                        if param.is_dots() {
                            Some(SmolStr::new("..."))
                        } else {
                            None
                        }
                    },
                    |name_token| Some(name_token.get_name_text().into()),
                )?;
                Some(SalsaSignatureParamSummary {
                    name,
                    syntax_offset: param.get_position(),
                    is_vararg: param.is_dots(),
                })
            })
            .collect();

        let return_expr_offsets = closure
            .get_block()
            .into_iter()
            .flat_map(analyze_signature_return_points)
            .map(|expr| expr.get_position())
            .collect();

        let doc_generic_offsets = self
            .doc
            .generics
            .iter()
            .filter(|generic| {
                generic.owner.kind == owner_kind
                    && generic.owner.syntax_offset == Some(owner_offset)
            })
            .map(|generic| generic.syntax_offset)
            .collect();
        let doc_param_offsets = self
            .doc
            .params
            .iter()
            .filter(|param| {
                param.owner.kind == owner_kind && param.owner.syntax_offset == Some(owner_offset)
            })
            .map(|param| param.syntax_offset)
            .collect();
        let doc_return_offsets = self
            .doc
            .returns
            .iter()
            .filter(|ret| {
                ret.owner.kind == owner_kind && ret.owner.syntax_offset == Some(owner_offset)
            })
            .map(|ret| ret.syntax_offset)
            .collect();
        let doc_operator_offsets = self
            .doc
            .operators
            .iter()
            .filter(|op| {
                op.owner.kind == owner_kind && op.owner.syntax_offset == Some(owner_offset)
            })
            .map(|op| op.syntax_offset)
            .collect();

        Some(SalsaSignatureSummary {
            syntax_offset: signature_offset,
            owner_offset,
            owner_kind,
            source,
            name,
            is_method,
            params,
            return_expr_offsets,
            doc_generic_offsets,
            doc_param_offsets,
            doc_return_offsets,
            doc_operator_offsets,
        })
    }
}

fn build_call_summary(call: LuaCallExpr) -> SalsaCallSummary {
    let arg_list = call.get_args_list();
    let kind = if call.is_require() {
        SalsaCallKindSummary::Require
    } else if call.is_error() {
        SalsaCallKindSummary::Error
    } else if call.is_assert() {
        SalsaCallKindSummary::Assert
    } else if call.is_type() {
        SalsaCallKindSummary::Type
    } else if call.is_setmetatable() {
        SalsaCallKindSummary::SetMetatable
    } else {
        SalsaCallKindSummary::Normal
    };

    SalsaCallSummary {
        syntax_offset: call.get_position(),
        syntax_id: call.get_syntax_id().into(),
        callee_offset: call
            .get_prefix_expr()
            .map(|expr| expr.get_position())
            .unwrap_or_else(|| TextSize::from(0)),
        kind,
        is_colon_call: call.is_colon_call(),
        is_single_arg_no_parens: arg_list
            .as_ref()
            .is_some_and(|arg_list| arg_list.is_single_arg_no_parens()),
        arg_expr_offsets: arg_list
            .as_ref()
            .into_iter()
            .flat_map(|arg_list| arg_list.get_args())
            .map(|expr| expr.get_position())
            .collect(),
        call_generic_type_offsets: call
            .get_call_generic_type_list()
            .into_iter()
            .flat_map(|type_list| type_list.get_types())
            .map(SalsaDocTypeNodeKey::from)
            .collect(),
    }
}

fn extract_function_name(func_name: Option<LuaVarExpr>) -> Option<SmolStr> {
    match func_name? {
        LuaVarExpr::NameExpr(name_expr) => name_expr.get_name_text().map(Into::into),
        LuaVarExpr::IndexExpr(index_expr) => extract_index_name(&index_expr),
    }
}

fn extract_index_name(index_expr: &emmylua_parser::LuaIndexExpr) -> Option<SmolStr> {
    let prefix = match index_expr.get_prefix_expr()? {
        LuaExpr::NameExpr(name_expr) => name_expr.get_name_text()?.to_string(),
        LuaExpr::IndexExpr(parent) => extract_index_name(&parent)?.to_string(),
        LuaExpr::ParenExpr(paren_expr) => match paren_expr.get_expr()? {
            LuaExpr::NameExpr(name_expr) => name_expr.get_name_text()?.to_string(),
            LuaExpr::IndexExpr(parent) => extract_index_name(&parent)?.to_string(),
            _ => return None,
        },
        _ => return None,
    };
    let member = match index_expr.get_index_key()? {
        emmylua_parser::LuaIndexKey::Name(name) => name.get_name_text().to_string(),
        emmylua_parser::LuaIndexKey::String(string) => string.get_value(),
        _ => return None,
    };
    Some(format!("{prefix}.{member}").into())
}

fn is_method_func_stat(stat: &LuaFuncStat) -> bool {
    let Some(func_name) = stat.get_func_name() else {
        return false;
    };
    let LuaVarExpr::IndexExpr(index_expr) = func_name else {
        return false;
    };

    index_expr
        .get_index_token()
        .is_some_and(|index_token| index_token.is_colon())
}
