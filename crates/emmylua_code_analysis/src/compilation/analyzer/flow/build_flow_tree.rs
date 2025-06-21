use std::collections::HashMap;

use emmylua_parser::{
    LuaAst, LuaAstNode, LuaAstToken, LuaBlock, LuaBreakStat, LuaChunk, LuaComment, LuaDocTagCast,
    LuaExpr, LuaGotoStat, LuaIndexExpr, LuaLabelStat, LuaLoopStat, LuaNameExpr, LuaStat,
    LuaSyntaxKind, LuaTokenKind, PathTrait,
};
use rowan::{TextRange, TextSize, WalkEvent};
use smol_str::SmolStr;

use crate::{
    AnalyzeError, DbIndex, DiagnosticCode, FileId, InFiled, LuaDeclId, LuaFlowId, LuaVarRefId,
    LuaVarRefNode,
};

use super::flow_node::{BlockId, FlowNode};

#[derive(Debug)]
pub struct LuaFlowTreeBuilder {
    current_flow_id: LuaFlowId,
    flow_id_stack: Vec<LuaFlowId>,
    flow_nodes: HashMap<LuaFlowId, FlowNode>,
    var_flow_ref: HashMap<LuaVarRefId, Vec<(LuaVarRefNode, LuaFlowId)>>,
    root_flow_id: LuaFlowId,
}

#[allow(unused)]
impl LuaFlowTreeBuilder {
    pub fn new(root: LuaChunk) -> LuaFlowTreeBuilder {
        let current_flow_id = LuaFlowId::from_chunk(root.clone());
        let mut builder = LuaFlowTreeBuilder {
            current_flow_id,
            flow_id_stack: Vec::new(),
            flow_nodes: HashMap::new(),
            var_flow_ref: HashMap::new(),
            root_flow_id: current_flow_id,
        };

        builder.flow_nodes.insert(
            current_flow_id,
            FlowNode::new(current_flow_id, current_flow_id.get_range(), None),
        );
        builder
    }

    pub fn enter_flow(&mut self, flow_id: LuaFlowId, range: TextRange) {
        let parent = self.current_flow_id;
        self.flow_id_stack.push(flow_id);
        self.current_flow_id = flow_id;
        self.flow_nodes
            .insert(flow_id, FlowNode::new(flow_id, range, Some(parent)));
        if let Some(parent_tree) = self.flow_nodes.get_mut(&parent) {
            parent_tree.add_child(flow_id);
        }
    }

    pub fn pop_flow(&mut self) {
        self.flow_id_stack.pop();
        self.current_flow_id = self
            .flow_id_stack
            .last()
            .unwrap_or(&self.root_flow_id)
            .clone();
    }

    pub fn add_flow_node(&mut self, ref_id: LuaVarRefId, ref_node: LuaVarRefNode) -> Option<()> {
        let flow_id = self.current_flow_id;
        self.var_flow_ref
            .entry(ref_id.clone())
            .or_insert_with(Vec::new)
            .push((ref_node.clone(), flow_id));

        Some(())
    }

    pub fn get_flow_node(&self, flow_id: LuaFlowId) -> Option<&FlowNode> {
        self.flow_nodes.get(&flow_id)
    }

    pub fn get_flow_node_mut(&mut self, flow_id: LuaFlowId) -> Option<&mut FlowNode> {
        self.flow_nodes.get_mut(&flow_id)
    }

    pub fn get_current_flow_node(&self) -> Option<&FlowNode> {
        self.flow_nodes.get(&self.current_flow_id)
    }

    pub fn get_current_flow_node_mut(&mut self) -> Option<&mut FlowNode> {
        self.flow_nodes.get_mut(&self.current_flow_id)
    }

    pub fn get_current_flow_id(&self) -> LuaFlowId {
        self.current_flow_id
    }

    pub fn get_var_ref_ids(&self) -> Vec<LuaVarRefId> {
        self.var_flow_ref.keys().cloned().collect()
    }

    pub fn get_flow_id_from_position(&self, position: TextSize) -> LuaFlowId {
        let mut result = self.root_flow_id;
        let mut stack = vec![self.root_flow_id];

        while let Some(flow_id) = stack.pop() {
            if let Some(node) = self.flow_nodes.get(&flow_id) {
                if node.get_range().contains(position) {
                    result = flow_id;
                    if node.get_children().is_empty() {
                        break;
                    }

                    stack.extend(node.get_children().iter().rev().copied());
                }
            }
        }

        result
    }

    pub fn get_var_ref_nodes(
        &self,
        var_ref_id: &LuaVarRefId,
    ) -> Option<&Vec<(LuaVarRefNode, LuaFlowId)>> {
        self.var_flow_ref.get(var_ref_id)
    }
}

pub fn build_flow_tree(db: &mut DbIndex, file_id: FileId, root: LuaChunk) -> LuaFlowTreeBuilder {
    let mut flow_tree = LuaFlowTreeBuilder::new(root.clone());
    let mut goto_vecs: Vec<(LuaFlowId, LuaGotoStat)> = vec![];
    for walk_node in root.walk_descendants::<LuaAst>() {
        match walk_node {
            WalkEvent::Enter(node) => match node {
                LuaAst::LuaClosureExpr(closure) => {
                    flow_tree.enter_flow(
                        LuaFlowId::from_closure(closure.clone()),
                        closure.get_range(),
                    );
                }
                LuaAst::LuaNameExpr(name_expr) => {
                    build_name_expr_flow(db, &mut flow_tree, file_id, name_expr);
                }
                LuaAst::LuaIndexExpr(index_expr) => {
                    build_index_expr_flow(db, &mut flow_tree, file_id, index_expr);
                }
                LuaAst::LuaDocTagCast(cast) => {
                    build_cast_flow(db, &mut flow_tree, file_id, cast);
                }
                LuaAst::LuaLabelStat(label) => {
                    build_label_flow(db, &mut flow_tree, file_id, label);
                }
                LuaAst::LuaGotoStat(goto_stat) => {
                    let current_flow_id = flow_tree.get_current_flow_id();
                    goto_vecs.push((current_flow_id, goto_stat.clone()));
                }
                LuaAst::LuaBreakStat(break_stat) => {
                    build_break_flow(db, &mut flow_tree, file_id, break_stat);
                }
                _ => {}
            },
            WalkEvent::Leave(node) => match node {
                LuaAst::LuaClosureExpr(_) => flow_tree.pop_flow(),
                _ => {}
            },
        }
    }

    for (flow_id, goto_stat) in goto_vecs {
        build_goto_flow(db, &mut flow_tree, file_id, goto_stat, flow_id);
    }

    flow_tree
}

fn build_name_expr_flow(
    db: &DbIndex,
    builder: &mut LuaFlowTreeBuilder,
    file_id: FileId,
    name_expr: LuaNameExpr,
) -> Option<()> {
    let parent = name_expr.get_parent::<LuaAst>()?;
    let mut is_assign = false;
    match &parent {
        LuaAst::LuaIndexExpr(index_expr) => {
            let parent = index_expr.get_parent::<LuaAst>()?;
            if parent.syntax().kind() != LuaSyntaxKind::CallExpr.into() {
                return None;
            }
        }
        LuaAst::LuaCallExpr(_) | LuaAst::LuaFuncStat(_) => return None,
        LuaAst::LuaAssignStat(assign_stat) => {
            let eq_pos = assign_stat
                .token_by_kind(LuaTokenKind::TkAssign)?
                .get_position();
            let decl_id = LuaDeclId::new(file_id, name_expr.get_position());
            if db.get_decl_index().get_decl(&decl_id).is_some() {
                return None;
            }

            if name_expr.get_position() < eq_pos {
                is_assign = true;
            }
        }
        _ => {}
    }
    let mut ref_id: Option<LuaVarRefId> = None;
    if let Some(local_refs) = db.get_reference_index().get_local_reference(&file_id) {
        if let Some(decl_id) = local_refs.get_decl_id(&name_expr.get_range()) {
            if let Some(decl) = db.get_decl_index().get_decl(&decl_id) {
                // 处理`self`作为参数传入的特殊情况
                if decl.is_param()
                    && name_expr
                        .get_name_text()
                        .map_or(false, |name| name == "self")
                {
                    ref_id = Some(LuaVarRefId::Name(SmolStr::new("self")));
                } else {
                    ref_id = Some(LuaVarRefId::DeclId(decl_id.clone()));
                }
            } else {
                ref_id = Some(LuaVarRefId::DeclId(decl_id.clone()));
            }
        }
    }

    if ref_id.is_none() {
        ref_id = Some(LuaVarRefId::Name(SmolStr::new(&name_expr.get_name_text()?)));
    }

    let ref_id = ref_id?;
    if is_assign {
        builder.add_flow_node(ref_id, LuaVarRefNode::AssignRef(name_expr.into()));
    } else {
        builder.add_flow_node(ref_id, LuaVarRefNode::UseRef(name_expr.into()));
    }

    Some(())
}

fn build_index_expr_flow(
    db: &DbIndex,
    builder: &mut LuaFlowTreeBuilder,
    file_id: FileId,
    index_expr: LuaIndexExpr,
) -> Option<()> {
    let parent = index_expr.get_parent::<LuaAst>()?;
    let mut is_assign = false;
    match parent {
        LuaAst::LuaIndexExpr(index_expr) => {
            let parent = index_expr.get_parent::<LuaAst>()?;
            if parent.syntax().kind() != LuaSyntaxKind::CallExpr.into() {
                return None;
            }
        }
        LuaAst::LuaCallExpr(_) | LuaAst::LuaFuncStat(_) => return None,
        LuaAst::LuaAssignStat(assign_stat) => {
            let eq_pos = assign_stat
                .token_by_kind(LuaTokenKind::TkAssign)?
                .get_position();

            let decl_id = LuaDeclId::new(file_id, index_expr.get_position());
            if db.get_decl_index().get_decl(&decl_id).is_some() {
                return None;
            }

            if index_expr.get_position() < eq_pos {
                is_assign = true;
            }
        }
        _ => {}
    }

    let ref_id = LuaVarRefId::Name(SmolStr::new(&index_expr.get_access_path()?));
    if is_assign {
        builder.add_flow_node(ref_id, LuaVarRefNode::AssignRef(index_expr.into()));
    } else {
        builder.add_flow_node(ref_id, LuaVarRefNode::UseRef(index_expr.into()));
    }

    Some(())
}

fn build_cast_flow(
    db: &DbIndex,
    builder: &mut LuaFlowTreeBuilder,
    file_id: FileId,
    tag_cast: LuaDocTagCast,
) -> Option<()> {
    match tag_cast.get_key_expr() {
        Some(target_expr) => {
            let text = match &target_expr {
                LuaExpr::NameExpr(name_expr) => name_expr.get_name_text()?,
                LuaExpr::IndexExpr(index_expr) => index_expr.get_access_path()?,
                _ => {
                    return None;
                }
            };

            let decl_tree = db.get_decl_index().get_decl_tree(&file_id)?;
            if let Some(decl) = decl_tree.find_local_decl(&text, target_expr.get_position()) {
                let decl_id = decl.get_id();
                builder.add_flow_node(
                    LuaVarRefId::DeclId(decl_id),
                    LuaVarRefNode::CastRef(tag_cast.clone()),
                );
            } else {
                let ref_id = LuaVarRefId::Name(SmolStr::new(text));
                if db
                    .get_decl_index()
                    .get_decl(&LuaDeclId::new(file_id, target_expr.get_position()))
                    .is_none()
                {
                    builder.add_flow_node(ref_id, LuaVarRefNode::CastRef(tag_cast.clone()));
                }
            }
        }
        None => {
            // 没有指定名称, 则附加到最近的表达式上
            let comment = tag_cast.get_parent::<LuaComment>()?;
            let mut left_token = comment.syntax().first_token()?.prev_token()?;
            if left_token.kind() == LuaTokenKind::TkWhitespace.into() {
                left_token = left_token.prev_token()?;
            }

            let mut ast_node = left_token.parent()?;
            loop {
                if LuaExpr::can_cast(ast_node.kind().into()) {
                    break;
                } else if LuaBlock::can_cast(ast_node.kind().into()) {
                    return None;
                }
                ast_node = ast_node.parent()?;
            }
            let expr = LuaExpr::cast(ast_node)?;
            let in_filed_syntax_id = InFiled::new(file_id, expr.get_syntax_id());
            builder.add_flow_node(
                LuaVarRefId::SyntaxId(in_filed_syntax_id),
                LuaVarRefNode::CastRef(tag_cast.clone()),
            );
        }
    }
    Some(())
}

fn build_label_flow(
    db: &mut DbIndex,
    builder: &mut LuaFlowTreeBuilder,
    file_id: FileId,
    label: LuaLabelStat,
) -> Option<()> {
    let decl_id = LuaDeclId::new(file_id, label.get_position());
    if db.get_decl_index().get_decl(&decl_id).is_some() {
        return None;
    }

    let flow_tree = builder.get_current_flow_node_mut()?;
    let label_token = label.get_label_name_token()?;
    let label_name = label_token.get_name_text();
    let block = label.get_parent::<LuaBlock>()?;
    let block_id = BlockId::from_block(block);
    if flow_tree.is_exist_label_in_same_block(label_name, block_id) {
        db.get_diagnostic_index_mut().add_diagnostic(
            file_id,
            AnalyzeError::new(
                DiagnosticCode::SyntaxError,
                &t!(
                    "Label `%{name}` already exists in the same block",
                    name = label_name
                ),
                label.get_range(),
            ),
        );
        return None;
    }

    flow_tree.add_label_ref(label_name, label);
    Some(())
}

fn build_goto_flow(
    db: &mut DbIndex,
    builder: &mut LuaFlowTreeBuilder,
    file_id: FileId,
    goto_stat: LuaGotoStat,
    flow_id: LuaFlowId,
) -> Option<()> {
    let flow_node = builder.get_flow_node_mut(flow_id)?;
    let label_token = goto_stat.get_label_name_token()?;
    let label_name = label_token.get_name_text();
    let label = flow_node.find_label(label_name, goto_stat.clone());
    if label.is_none() {
        db.get_diagnostic_index_mut().add_diagnostic(
            file_id,
            AnalyzeError::new(
                DiagnosticCode::SyntaxError,
                &t!("Label `%{name}` not found", name = label_name),
                label_token.get_range(),
            ),
        );
    }

    let label = label?;

    flow_node.add_jump_to_stat(
        goto_stat.get_syntax_id(),
        LuaStat::cast(label.syntax().clone())?,
    );

    Some(())
}

fn build_break_flow(
    db: &mut DbIndex,
    builder: &mut LuaFlowTreeBuilder,
    file_id: FileId,
    break_stat: LuaBreakStat,
) -> Option<()> {
    let flow_tree = builder.get_current_flow_node_mut()?;
    let first_loop_stat = break_stat.ancestors::<LuaLoopStat>().next();
    if first_loop_stat.is_none() {
        db.get_diagnostic_index_mut().add_diagnostic(
            file_id,
            AnalyzeError::new(
                DiagnosticCode::SyntaxError,
                &t!("`break` statement not in a loop"),
                break_stat.get_range(),
            ),
        );
        return None;
    }
    let loop_stat = first_loop_stat?;
    flow_tree.add_jump_to_stat(
        break_stat.get_syntax_id(),
        LuaStat::cast(loop_stat.syntax().clone())?,
    );

    Some(())
}
