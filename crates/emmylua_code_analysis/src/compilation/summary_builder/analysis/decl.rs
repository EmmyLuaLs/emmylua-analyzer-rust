use std::collections::BTreeMap;

use emmylua_parser::{
    LuaAssignStat, LuaAst, LuaAstNode, LuaAstToken, LuaChunk, LuaClosureExpr, LuaExpr,
    LuaForRangeStat, LuaForStat, LuaFuncStat, LuaIndexExpr, LuaIndexKey, LuaLocalFuncStat,
    LuaLocalStat, LuaSyntaxKind, LuaVarExpr, NumberResult,
};
use rowan::{TextRange, TextSize, WalkEvent};

use crate::{FileId, SalsaDeclId};
use smol_str::SmolStr;

use super::super::{
    SalsaDeclKindSummary, SalsaDeclSummary, SalsaDeclTreeSummary, SalsaGlobalEntrySummary,
    SalsaGlobalFunctionSummary, SalsaGlobalRootSummary, SalsaGlobalSummary,
    SalsaGlobalVariableSummary, SalsaLocalAttributeSummary, SalsaMemberIndexSummary,
    SalsaMemberKindSummary, SalsaMemberRootSummary, SalsaMemberSummary, SalsaScopeChildSummary,
    SalsaScopeKindSummary, SalsaScopeSummary, SalsaSyntaxIdSummary,
};
use crate::compilation::summary_builder::summary::SalsaMemberTargetSummary;

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(crate) struct SummaryDeclAnalysis {
    pub decl_tree: SalsaDeclTreeSummary,
    pub globals: SalsaGlobalSummary,
    pub members: SalsaMemberIndexSummary,
}

pub(crate) fn analyze_decl_summary(file_id: FileId, chunk: LuaChunk) -> SummaryDeclAnalysis {
    SummaryDeclAnalyzer::new(file_id, chunk).analyze()
}

struct SummaryDeclAnalyzer {
    file_id: FileId,
    root: LuaChunk,
    decls: Vec<SalsaDeclSummary>,
    scopes: Vec<SalsaScopeSummary>,
    scope_stack: Vec<u32>,
    global_entries: BTreeMap<SmolStr, Vec<SalsaDeclId>>,
    global_variables: Vec<SalsaGlobalVariableSummary>,
    global_functions: Vec<SalsaGlobalFunctionSummary>,
    global_members: Vec<SalsaMemberSummary>,
    local_members: Vec<SalsaMemberSummary>,
}

impl SummaryDeclAnalyzer {
    fn new(file_id: FileId, root: LuaChunk) -> Self {
        Self {
            file_id,
            root,
            decls: Vec::new(),
            scopes: Vec::new(),
            scope_stack: Vec::new(),
            global_entries: BTreeMap::new(),
            global_variables: Vec::new(),
            global_functions: Vec::new(),
            global_members: Vec::new(),
            local_members: Vec::new(),
        }
    }

    fn analyze(mut self) -> SummaryDeclAnalysis {
        let root = self.root.clone();
        for walk_event in root.walk_descendants::<LuaAst>() {
            match walk_event {
                WalkEvent::Enter(node) => self.walk_node_enter(node),
                WalkEvent::Leave(node) => self.walk_node_leave(node),
            }
        }

        SummaryDeclAnalysis {
            decl_tree: SalsaDeclTreeSummary {
                file_id: self.file_id,
                decls: self.decls,
                scopes: self.scopes,
            },
            globals: SalsaGlobalSummary {
                entries: self
                    .global_entries
                    .into_iter()
                    .map(|(name, decl_ids)| SalsaGlobalEntrySummary { name, decl_ids })
                    .collect(),
                variables: self.global_variables,
                functions: self.global_functions,
                members: self.global_members.clone(),
            },
            members: SalsaMemberIndexSummary {
                members: self
                    .global_members
                    .into_iter()
                    .chain(self.local_members)
                    .collect(),
            },
        }
    }

    fn walk_node_enter(&mut self, node: LuaAst) {
        match node {
            LuaAst::LuaChunk(chunk) => {
                self.create_scope(chunk.get_range(), SalsaScopeKindSummary::Normal);
            }
            LuaAst::LuaBlock(block) => {
                self.create_scope(block.get_range(), SalsaScopeKindSummary::Normal);
            }
            LuaAst::LuaLocalStat(stat) => {
                self.create_scope(stat.get_range(), SalsaScopeKindSummary::LocalOrAssignStat);
                self.analyze_local_stat(stat);
            }
            LuaAst::LuaAssignStat(stat) => {
                self.create_scope(stat.get_range(), SalsaScopeKindSummary::LocalOrAssignStat);
                self.analyze_assign_stat(stat);
            }
            LuaAst::LuaForStat(stat) => {
                self.create_scope(stat.get_range(), SalsaScopeKindSummary::Normal);
                self.analyze_for_stat(stat);
            }
            LuaAst::LuaForRangeStat(stat) => {
                self.create_scope(stat.get_range(), SalsaScopeKindSummary::ForRange);
                self.analyze_for_range_stat(stat);
            }
            LuaAst::LuaFuncStat(stat) => {
                let kind = if is_method_func_stat(&stat) {
                    SalsaScopeKindSummary::MethodStat
                } else {
                    SalsaScopeKindSummary::FuncStat
                };
                self.create_scope(stat.get_range(), kind);
                self.analyze_func_stat(stat);
            }
            LuaAst::LuaLocalFuncStat(stat) => {
                self.create_scope(stat.get_range(), SalsaScopeKindSummary::FuncStat);
                self.analyze_local_func_stat(stat);
            }
            LuaAst::LuaRepeatStat(stat) => {
                self.create_scope(stat.get_range(), SalsaScopeKindSummary::Repeat);
            }
            LuaAst::LuaClosureExpr(expr) => {
                self.create_scope(expr.get_range(), SalsaScopeKindSummary::Normal);
                self.analyze_closure_expr(expr);
            }
            _ => {}
        }
    }

    fn walk_node_leave(&mut self, node: LuaAst) {
        if is_scope_owner(&node) {
            self.scope_stack.pop();
        }
    }

    fn create_scope(&mut self, range: TextRange, kind: SalsaScopeKindSummary) {
        let scope_id = self.scopes.len() as u32;
        let parent = self.scope_stack.last().copied();
        self.scopes.push(SalsaScopeSummary {
            id: scope_id,
            parent,
            kind,
            start_offset: range.start(),
            end_offset: range.end(),
            children: Vec::new(),
        });

        if let Some(parent_id) = parent {
            self.scopes[parent_id as usize]
                .children
                .push(SalsaScopeChildSummary::Scope(scope_id));
        }

        self.scope_stack.push(scope_id);
    }

    fn add_decl(
        &mut self,
        name: SmolStr,
        kind: SalsaDeclKindSummary,
        syntax_id: Option<SalsaSyntaxIdSummary>,
        range: TextRange,
        value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
        value_result_index: usize,
        source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
    ) -> SalsaDeclId {
        let scope_id = self.scope_stack.last().copied().unwrap_or_default();
        let decl_id = SalsaDeclId::new(range.start());
        self.decls.push(SalsaDeclSummary {
            id: decl_id,
            name: name.clone(),
            kind: kind.clone(),
            syntax_id,
            scope_id,
            start_offset: range.start(),
            end_offset: range.end(),
            value_expr_syntax_id,
            value_result_index,
            source_call_syntax_id,
        });
        self.scopes[scope_id as usize]
            .children
            .push(SalsaScopeChildSummary::Decl(decl_id));

        if matches!(kind, SalsaDeclKindSummary::Global) {
            self.global_entries.entry(name).or_default().push(decl_id);
        }

        decl_id
    }

    fn find_visible_decl(&self, name: &str) -> Option<&SalsaDeclSummary> {
        for scope_id in self.scope_stack.iter().rev() {
            let scope = self.scopes.get(*scope_id as usize)?;
            for child in scope.children.iter().rev() {
                if let SalsaScopeChildSummary::Decl(decl_id) = child
                    && let Some(decl) = self.decls.iter().rev().find(|decl| decl.id == *decl_id)
                    && decl.name == name
                {
                    return Some(decl);
                }
            }
        }

        None
    }

    fn find_visible_decl_before_offset(
        &self,
        name: &str,
        offset: TextSize,
    ) -> Option<&SalsaDeclSummary> {
        self.find_visible_decl(name).or_else(|| {
            self.decls
                .iter()
                .rev()
                .find(|decl| decl.name == name && decl.start_offset <= offset)
        })
    }

    fn analyze_local_stat(&mut self, stat: LuaLocalStat) {
        let local_name_list = stat.get_local_name_list().collect::<Vec<_>>();
        let value_expr_list = stat.get_value_exprs().collect::<Vec<_>>();
        let last_expr_index = value_expr_list.len().saturating_sub(1);

        for (index, local_name) in local_name_list.iter().enumerate() {
            let Some(name_token) = local_name.get_name_token() else {
                continue;
            };

            let attrib = local_name.get_attrib().and_then(|attrib| {
                if attrib.is_const() {
                    Some(SalsaLocalAttributeSummary::Const)
                } else if attrib.is_close() {
                    Some(SalsaLocalAttributeSummary::Close)
                } else {
                    None
                }
            });

            let (value_expr, value_result_index) = if let Some(expr) = value_expr_list.get(index) {
                (Some(expr), 0)
            } else {
                (
                    value_expr_list.last(),
                    index.saturating_sub(last_expr_index),
                )
            };

            self.add_decl(
                name_token.get_name_text().into(),
                SalsaDeclKindSummary::Local { attrib },
                Some(local_name.get_syntax_id().into()),
                local_name.get_range(),
                value_expr.map(|expr| expr.get_syntax_id().into()),
                value_result_index,
                value_expr.and_then(Self::call_expr_syntax_id_of_expr),
            );
        }
    }

    fn call_expr_syntax_id_of_expr(expr: &LuaExpr) -> Option<SalsaSyntaxIdSummary> {
        match expr {
            LuaExpr::CallExpr(call_expr) => Some(call_expr.get_syntax_id().into()),
            LuaExpr::ParenExpr(paren_expr) => {
                Self::call_expr_syntax_id_of_expr(&paren_expr.get_expr()?)
            }
            _ => None,
        }
    }

    fn analyze_assign_stat(&mut self, stat: LuaAssignStat) {
        let (vars, value_exprs) = stat.get_var_and_expr_list();
        let last_expr_index = value_exprs.len().saturating_sub(1);
        for (idx, var) in vars.iter().enumerate() {
            let (value_expr, value_result_index) = if let Some(expr) = value_exprs.get(idx) {
                (Some(expr), 0)
            } else {
                (value_exprs.last(), idx.saturating_sub(last_expr_index))
            };
            let value_expr_syntax_id = value_expr.map(|expr| expr.get_syntax_id().into());
            let source_call_syntax_id = value_expr.and_then(Self::call_expr_syntax_id_of_expr);
            match var {
                LuaVarExpr::NameExpr(name_expr) => {
                    let Some(name_token) = name_expr.get_name_token() else {
                        continue;
                    };
                    let name = name_token.get_name_text();
                    if name == "_" {
                        let decl_id = self.add_decl(
                            name.into(),
                            SalsaDeclKindSummary::Local {
                                attrib: Some(SalsaLocalAttributeSummary::Const),
                            },
                            Some(name_expr.get_syntax_id().into()),
                            name_token.get_range(),
                            value_expr_syntax_id,
                            value_result_index,
                            source_call_syntax_id,
                        );
                        let _ = decl_id;
                        continue;
                    }

                    if self
                        .find_visible_decl_before_offset(name, name_token.get_position())
                        .is_none()
                    {
                        let decl_id = self.add_decl(
                            name.into(),
                            SalsaDeclKindSummary::Global,
                            Some(name_expr.get_syntax_id().into()),
                            name_token.get_range(),
                            value_expr_syntax_id,
                            value_result_index,
                            source_call_syntax_id,
                        );
                        self.record_global_assignment(
                            name.into(),
                            decl_id,
                            value_expr_syntax_id,
                            value_exprs.get(idx),
                        );
                    }
                }
                LuaVarExpr::IndexExpr(index_expr) => {
                    self.analyze_maybe_global_index_expr(
                        index_expr,
                        value_expr_syntax_id,
                        value_expr,
                        value_result_index,
                        source_call_syntax_id,
                    );
                    self.analyze_local_member_index_expr(
                        index_expr,
                        value_expr_syntax_id,
                        value_expr,
                        value_result_index,
                        source_call_syntax_id,
                        false,
                    );
                }
            }
        }
    }

    fn analyze_maybe_global_index_expr(
        &mut self,
        index_expr: &LuaIndexExpr,
        value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
        value_expr: Option<&LuaExpr>,
        value_result_index: usize,
        source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
    ) {
        let Some(path) = self.extract_global_path_from_index_expr(index_expr) else {
            return;
        };
        if path.len() == 1 {
            let name = path[0].clone();
            if self
                .find_visible_decl_before_offset(&name, index_expr.get_position())
                .is_none()
            {
                let decl_id = self.add_decl(
                    name.clone(),
                    SalsaDeclKindSummary::Global,
                    Some(index_expr.get_syntax_id().into()),
                    index_expr.get_range(),
                    value_expr_syntax_id,
                    0,
                    value_expr.and_then(Self::call_expr_syntax_id_of_expr),
                );
                self.record_global_assignment(name, decl_id, value_expr_syntax_id, value_expr);
            }
            return;
        }

        self.record_global_member(
            path,
            index_expr.get_syntax_id().into(),
            value_expr_syntax_id,
            value_expr,
            value_result_index,
            source_call_syntax_id,
            false,
        );
    }

    fn analyze_for_stat(&mut self, stat: LuaForStat) {
        let Some(var) = stat.get_var_name() else {
            return;
        };
        self.add_decl(
            var.get_name_text().into(),
            SalsaDeclKindSummary::Local {
                attrib: Some(SalsaLocalAttributeSummary::IterConst),
            },
            Some(var.get_syntax_id().into()),
            var.get_range(),
            None,
            0,
            None,
        );
    }

    fn analyze_for_range_stat(&mut self, stat: LuaForRangeStat) {
        for var in stat.get_var_name_list() {
            self.add_decl(
                var.get_name_text().into(),
                SalsaDeclKindSummary::Local {
                    attrib: Some(SalsaLocalAttributeSummary::IterConst),
                },
                Some(var.get_syntax_id().into()),
                var.get_range(),
                None,
                0,
                None,
            );
        }
    }

    fn analyze_func_stat(&mut self, stat: LuaFuncStat) {
        let Some(func_name) = stat.get_func_name() else {
            return;
        };
        let Some(closure) = stat.get_closure() else {
            return;
        };
        let signature_offset = closure.get_position();
        let signature_syntax_id = Some(closure.get_syntax_id().into());

        match func_name {
            LuaVarExpr::NameExpr(name_expr) => {
                let Some(name_token) = name_expr.get_name_token() else {
                    return;
                };
                let name = name_token.get_name_text();
                if self
                    .find_visible_decl_before_offset(name, name_token.get_position())
                    .is_none()
                {
                    let decl_id = self.add_decl(
                        name.into(),
                        SalsaDeclKindSummary::Global,
                        Some(name_expr.get_syntax_id().into()),
                        name_token.get_range(),
                        signature_syntax_id,
                        0,
                        None,
                    );
                    self.global_functions.push(SalsaGlobalFunctionSummary {
                        name: name.into(),
                        decl_id: Some(decl_id),
                        signature_offset,
                        is_method: false,
                    });
                }
            }
            LuaVarExpr::IndexExpr(index_expr) => {
                let Some(path) = self.extract_global_path_from_index_expr(&index_expr) else {
                    self.record_local_function_member(&index_expr, closure.clone());
                    return;
                };
                if path.len() == 1 {
                    let name = path[0].clone();
                    if self
                        .find_visible_decl_before_offset(&name, index_expr.get_position())
                        .is_none()
                    {
                        let decl_id = self.add_decl(
                            name.clone(),
                            SalsaDeclKindSummary::Global,
                            Some(index_expr.get_syntax_id().into()),
                            index_expr.get_range(),
                            signature_syntax_id,
                            0,
                            None,
                        );
                        self.global_functions.push(SalsaGlobalFunctionSummary {
                            name,
                            decl_id: Some(decl_id),
                            signature_offset,
                            is_method: index_expr
                                .get_index_token()
                                .is_some_and(|index_token| index_token.is_colon()),
                        });
                    }
                } else {
                    self.record_global_member(
                        path,
                        index_expr.get_syntax_id().into(),
                        Some(closure.get_syntax_id().into()),
                        Some(&LuaExpr::ClosureExpr(closure.clone())),
                        0,
                        None,
                        index_expr
                            .get_index_token()
                            .is_some_and(|index_token| index_token.is_colon()),
                    );
                }

                self.record_local_function_member(&index_expr, closure);
            }
        }
    }

    fn record_local_function_member(&mut self, index_expr: &LuaIndexExpr, closure: LuaClosureExpr) {
        let is_method = index_expr
            .get_index_token()
            .is_some_and(|index_token| index_token.is_colon());
        self.analyze_local_member_index_expr(
            index_expr,
            Some(closure.get_syntax_id().into()),
            Some(&LuaExpr::ClosureExpr(closure)),
            0,
            None,
            is_method,
        );
    }

    fn analyze_local_func_stat(&mut self, stat: LuaLocalFuncStat) {
        let Some(local_name) = stat.get_local_name() else {
            return;
        };
        let Some(name_token) = local_name.get_name_token() else {
            return;
        };
        let Some(closure) = stat.get_closure() else {
            return;
        };

        self.add_decl(
            name_token.get_name_text().into(),
            SalsaDeclKindSummary::Local { attrib: None },
            Some(local_name.get_syntax_id().into()),
            local_name.get_range(),
            Some(closure.get_syntax_id().into()),
            0,
            None,
        );
    }

    fn analyze_closure_expr(&mut self, expr: LuaClosureExpr) {
        self.try_add_self_param(&expr);

        let Some(params) = expr.get_params_list() else {
            return;
        };
        let signature_offset = expr.get_position();
        for (idx, param) in params.get_params().enumerate() {
            let name = param.get_name_token().map_or_else(
                || {
                    if param.is_dots() {
                        SmolStr::new("...")
                    } else {
                        SmolStr::new("")
                    }
                },
                |name_token| name_token.get_name_text().into(),
            );
            if name.is_empty() {
                continue;
            }

            self.add_decl(
                name,
                SalsaDeclKindSummary::Param {
                    idx,
                    signature_offset,
                },
                Some(param.get_syntax_id().into()),
                param.get_range(),
                None,
                0,
                None,
            );
        }
    }

    fn try_add_self_param(&mut self, closure: &LuaClosureExpr) {
        let Some(func_stat) = closure.get_parent::<LuaFuncStat>() else {
            return;
        };
        let Some(func_name) = func_stat.get_func_name() else {
            return;
        };
        let LuaVarExpr::IndexExpr(index_expr) = func_name else {
            return;
        };
        let Some(index_token) = index_expr.get_index_token() else {
            return;
        };
        if !index_token.is_colon() {
            return;
        }

        self.add_decl(
            "self".into(),
            SalsaDeclKindSummary::ImplicitSelf,
            None,
            index_token.get_range(),
            None,
            0,
            None,
        );
    }

    fn record_global_assignment(
        &mut self,
        name: SmolStr,
        decl_id: SalsaDeclId,
        value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
        value_expr: Option<&LuaExpr>,
    ) {
        if let Some(LuaExpr::ClosureExpr(closure)) = value_expr {
            self.global_functions.push(SalsaGlobalFunctionSummary {
                name,
                decl_id: Some(decl_id),
                signature_offset: closure.get_position(),
                is_method: false,
            });
        } else {
            self.global_variables.push(SalsaGlobalVariableSummary {
                name,
                decl_id,
                value_expr_syntax_id,
            });
        }
    }

    fn record_global_member(
        &mut self,
        path: Vec<SmolStr>,
        syntax_id: SalsaSyntaxIdSummary,
        value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
        value_expr: Option<&LuaExpr>,
        value_result_index: usize,
        source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
        is_method: bool,
    ) {
        let Some(root_name) = path.first().cloned() else {
            return;
        };
        let Some(member_name) = path.last().cloned() else {
            return;
        };
        let root = if root_name == "_ENV" {
            SalsaGlobalRootSummary::Env
        } else {
            SalsaGlobalRootSummary::Name(root_name)
        };
        let owner_segments = if path.len() > 2 {
            path[1..path.len() - 1].to_vec()
        } else {
            Vec::new()
        };
        let kind = if is_method {
            SalsaMemberKindSummary::Method
        } else if matches!(value_expr, Some(LuaExpr::ClosureExpr(_))) {
            SalsaMemberKindSummary::Function
        } else {
            SalsaMemberKindSummary::Variable
        };

        self.global_members.push(SalsaMemberSummary {
            syntax_id,
            target: SalsaMemberTargetSummary {
                root: SalsaMemberRootSummary::Global(root),
                owner_segments: owner_segments.into(),
                member_name,
            }
            .into(),
            kind,
            signature_offset: match value_expr {
                Some(LuaExpr::ClosureExpr(closure)) => Some(closure.get_position()),
                _ => None,
            },
            value_expr_syntax_id,
            value_result_index,
            source_call_syntax_id,
            is_method,
        });
    }

    fn analyze_local_member_index_expr(
        &mut self,
        index_expr: &LuaIndexExpr,
        value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
        value_expr: Option<&LuaExpr>,
        value_result_index: usize,
        source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
        is_method: bool,
    ) {
        let Some(target) = self.extract_local_member_target_from_index_expr(index_expr) else {
            return;
        };
        let kind = if is_method {
            SalsaMemberKindSummary::Method
        } else if matches!(value_expr, Some(LuaExpr::ClosureExpr(_))) {
            SalsaMemberKindSummary::Function
        } else {
            SalsaMemberKindSummary::Variable
        };
        let signature_offset = match value_expr {
            Some(LuaExpr::ClosureExpr(closure)) => Some(closure.get_position()),
            _ => None,
        };
        self.local_members.push(SalsaMemberSummary {
            syntax_id: index_expr.get_syntax_id().into(),
            target: target.into(),
            kind,
            signature_offset,
            value_expr_syntax_id,
            value_result_index,
            source_call_syntax_id,
            is_method,
        });
    }

    fn extract_local_member_target_from_index_expr(
        &self,
        index_expr: &LuaIndexExpr,
    ) -> Option<SalsaMemberTargetSummary> {
        let (root, mut segments) =
            self.extract_local_member_root_and_segments(&index_expr.get_prefix_expr()?)?;
        segments.push(get_global_name_from_index_expr(index_expr)?);
        let member_name = segments.pop()?;
        Some(SalsaMemberTargetSummary {
            root,
            owner_segments: segments.into(),
            member_name,
        })
    }

    fn extract_local_member_root_and_segments(
        &self,
        expr: &LuaExpr,
    ) -> Option<(SalsaMemberRootSummary, Vec<SmolStr>)> {
        match expr {
            LuaExpr::NameExpr(name_expr) => {
                let name = name_expr.get_name_text()?;
                let decl = self.find_visible_decl_before_offset(&name, expr.get_position())?;
                if matches!(decl.kind, SalsaDeclKindSummary::Global) {
                    return None;
                }
                Some((
                    SalsaMemberRootSummary::LocalDecl {
                        name: name.into(),
                        decl_id: decl.id,
                    },
                    Vec::new(),
                ))
            }
            LuaExpr::IndexExpr(index_expr) => {
                let (root, mut segments) =
                    self.extract_local_member_root_and_segments(&index_expr.get_prefix_expr()?)?;
                segments.push(get_global_name_from_index_expr(index_expr)?);
                Some((root, segments))
            }
            LuaExpr::ParenExpr(paren_expr) => {
                self.extract_local_member_root_and_segments(&paren_expr.get_expr()?)
            }
            _ => None,
        }
    }

    fn extract_global_path_from_index_expr(
        &self,
        index_expr: &LuaIndexExpr,
    ) -> Option<Vec<SmolStr>> {
        let mut path = self.extract_global_path_from_expr(&index_expr.get_prefix_expr()?)?;
        let key = get_global_name_from_index_expr(index_expr)?;
        path.push(key);
        Some(path)
    }

    fn extract_global_path_from_expr(&self, expr: &LuaExpr) -> Option<Vec<SmolStr>> {
        match expr {
            LuaExpr::NameExpr(name_expr) => {
                let name = name_expr.get_name_text()?;
                if name == "_ENV" {
                    return Some(vec!["_ENV".into()]);
                }
                if name == "_G" {
                    return Some(Vec::new());
                }
                if self
                    .find_visible_decl_before_offset(&name, expr.get_position())
                    .is_some()
                {
                    return None;
                }
                Some(vec![name.into()])
            }
            LuaExpr::IndexExpr(index_expr) => self.extract_global_path_from_index_expr(index_expr),
            _ => None,
        }
    }
}

fn get_global_name_from_index_expr(index_expr: &LuaIndexExpr) -> Option<SmolStr> {
    let index_key = index_expr.get_index_key()?;
    match index_key {
        LuaIndexKey::Name(name) => Some(name.get_name_text().into()),
        LuaIndexKey::String(string) => Some(string.get_value().into()),
        LuaIndexKey::Integer(number) => match number.get_number_value() {
            NumberResult::Int(value) => Some(SmolStr::new(value.to_string())),
            _ => None,
        },
        _ => None,
    }
}

fn is_scope_owner(node: &LuaAst) -> bool {
    matches!(
        node.syntax().kind().into(),
        LuaSyntaxKind::Chunk
            | LuaSyntaxKind::Block
            | LuaSyntaxKind::ClosureExpr
            | LuaSyntaxKind::RepeatStat
            | LuaSyntaxKind::ForRangeStat
            | LuaSyntaxKind::ForStat
            | LuaSyntaxKind::LocalStat
            | LuaSyntaxKind::FuncStat
            | LuaSyntaxKind::LocalFuncStat
            | LuaSyntaxKind::AssignStat
    )
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
