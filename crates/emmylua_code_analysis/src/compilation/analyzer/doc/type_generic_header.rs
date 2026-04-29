use emmylua_parser::{LuaAstNode, LuaDocGenericDeclList, LuaDocType};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;

use crate::{
    FileId, GenericParam, GenericTplId,
    compilation::analyzer::AnalyzeContext,
    db_index::{DbIndex, LuaType, WorkspaceId},
    semantic::complete_type_generic_args_in_type,
};

use super::{
    file_generic_index::{GenericIndex, GenericScopeId},
    infer_type::{DocTypeAnalyzeContext, DocTypeAnalyzeOptions, infer_type},
};

pub(super) fn preprocess_type_generic_headers(db: &mut DbIndex, context: &mut AnalyzeContext) {
    let pending_headers = context.take_pending_type_generic_headers();
    if pending_headers.is_empty() {
        return;
    }

    let workspace_id = context.workspace_id.unwrap_or(WorkspaceId::MAIN);
    let mut resolved_headers = Vec::new();
    for pending_header in pending_headers {
        let type_id = pending_header.type_id;
        let params = resolve_generic_params(
            db,
            pending_header.file_id,
            workspace_id,
            pending_header.generic_decl_list,
        );

        resolved_headers.push((type_id, params));
    }

    // 先写入 raw metadata, 让后续 normalize 能跨类型看到所有泛型默认值.
    for (type_id, params) in &resolved_headers {
        if !params.is_empty() {
            db.get_type_index_mut()
                .add_generic_params(type_id.clone(), params.clone());
        }
    }

    // 再补齐 constraint/default 类型表达式里的泛型实参并覆盖 raw metadata.
    for (type_id, params) in resolved_headers {
        let normalized_params = normalize_generic_params(db, &params);
        if !normalized_params.is_empty() {
            db.get_type_index_mut()
                .add_generic_params(type_id, normalized_params);
        }
    }
}

fn normalize_generic_params(db: &DbIndex, params: &[GenericParam]) -> Vec<GenericParam> {
    params
        .iter()
        .map(|param| {
            GenericParam::new(
                param.name.clone(),
                param
                    .type_constraint
                    .as_ref()
                    .map(|ty| complete_type_generic_args_in_type(db, ty)),
                param
                    .default_type
                    .as_ref()
                    .map(|ty| complete_type_generic_args_in_type(db, ty)),
                param.attributes.clone(),
            )
        })
        .collect()
}

fn resolve_generic_params(
    db: &mut DbIndex,
    file_id: FileId,
    workspace_id: WorkspaceId,
    generic_decl_list: LuaDocGenericDeclList,
) -> Vec<GenericParam> {
    let mut generic_index = HeaderGenericIndex::new();
    let scope_id = generic_index.add_generic_scope(vec![generic_decl_list.get_range()], false);
    let mut params = Vec::new();

    for generic_decl in generic_decl_list.get_generic_decl() {
        let Some(name_token) = generic_decl.get_name_token() else {
            continue;
        };

        let name = SmolStr::new(name_token.get_name_text());
        let constraint = generic_decl.get_constraint_type().map(|type_ref| {
            infer_header_type(db, file_id, workspace_id, &mut generic_index, type_ref)
        });

        let default_type = generic_decl.get_default_type().map(|type_ref| {
            infer_header_type(db, file_id, workspace_id, &mut generic_index, type_ref)
        });

        let param = GenericParam::new(name, constraint, default_type, None);
        generic_index.append_generic_param(scope_id, param.clone());
        params.push(param);
    }

    params
}

fn infer_header_type(
    db: &mut DbIndex,
    file_id: FileId,
    workspace_id: WorkspaceId,
    generic_index: &mut HeaderGenericIndex,
    type_ref: LuaDocType,
) -> LuaType {
    let mut context = DocTypeAnalyzeContext::new(db, file_id, generic_index, workspace_id)
        .with_options(DocTypeAnalyzeOptions::header_preprocess());
    infer_type(&mut context, type_ref)
}

#[derive(Debug, Default)]
struct HeaderGenericIndex {
    scopes: Vec<HeaderGenericScope>,
}

impl HeaderGenericIndex {
    fn new() -> Self {
        Self::default()
    }

    fn next_index(&self, position: TextSize, is_func: bool) -> usize {
        self.scopes
            .iter()
            .filter(|scope| scope.next_tpl_id.is_func() == is_func && scope.contains(position))
            .map(|scope| scope.params.len())
            .sum()
    }

    fn next_tpl_id(&self, position: TextSize, is_func: bool) -> GenericTplId {
        generic_tpl_id(is_func, self.next_index(position, is_func) as u32)
    }
}

impl GenericIndex for HeaderGenericIndex {
    fn add_generic_scope(&mut self, ranges: Vec<TextRange>, is_func: bool) -> GenericScopeId {
        let next_tpl_id = ranges
            .first()
            .map(|range| self.next_tpl_id(range.start(), is_func))
            .unwrap_or_else(|| generic_tpl_id(is_func, 0));
        let id = GenericScopeId {
            id: self.scopes.len(),
        };
        self.scopes.push(HeaderGenericScope {
            ranges,
            next_tpl_id,
            params: Vec::new(),
        });
        id
    }

    fn append_generic_param(&mut self, scope_id: GenericScopeId, param: GenericParam) {
        let Some(scope) = self.scopes.get_mut(scope_id.id) else {
            return;
        };
        let tpl_id = scope.next_tpl_id;
        scope.next_tpl_id = scope.next_tpl_id.with_idx((tpl_id.get_idx() + 1) as u32);
        scope.params.push((tpl_id, param));
    }

    fn find_generic(
        &self,
        position: TextSize,
        name: &str,
    ) -> Option<(GenericTplId, Option<LuaType>, Option<LuaType>)> {
        for scope in self.scopes.iter().rev() {
            if !scope.contains(position) {
                continue;
            }

            if let Some((tpl_id, param)) = scope
                .params
                .iter()
                .rev()
                .find(|(_, param)| param.name == name)
            {
                return Some((
                    *tpl_id,
                    param.type_constraint.clone(),
                    param.default_type.clone(),
                ));
            }
        }

        None
    }
}

fn generic_tpl_id(is_func: bool, idx: u32) -> GenericTplId {
    if is_func {
        GenericTplId::Func(idx)
    } else {
        GenericTplId::Type(idx)
    }
}

#[derive(Debug)]
struct HeaderGenericScope {
    ranges: Vec<TextRange>,
    next_tpl_id: GenericTplId,
    params: Vec<(GenericTplId, GenericParam)>,
}

impl HeaderGenericScope {
    fn contains(&self, position: TextSize) -> bool {
        self.ranges
            .iter()
            .any(|range| range.contains(position) || range.start() == position)
    }
}
