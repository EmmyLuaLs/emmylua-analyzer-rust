mod attribute_tags;
mod diagnostic_tags;
mod field_or_operator_def_tags;
mod file_generic_index;
mod infer_type;
mod property_tags;
mod tags;
mod type_def_tags;
mod type_ref_tags;

use super::AnalyzeContext;
use crate::{
    FileId, LuaSemanticDeclId, WorkspaceId,
    compilation::analyzer::AnalysisPipeline,
    db_index::{DbIndex, LuaTypeDeclId},
    profile::Profile,
};
use emmylua_parser::{LuaAstNode, LuaComment, LuaSyntaxNode};
use file_generic_index::FileGenericIndex;
use tags::get_owner_id;
pub struct DocAnalysisPipeline;

impl AnalysisPipeline for DocAnalysisPipeline {
    fn analyze(db: &mut DbIndex, context: &mut AnalyzeContext) {
        let _p = Profile::cond_new("doc analyze", context.tree_list.len() > 1);
        let tree_list = context.tree_list.clone();
        let workspace_id = context.workspace_id.unwrap_or(WorkspaceId::MAIN);
        for in_filed_tree in tree_list.iter() {
            let root = &in_filed_tree.value;
            let mut generic_index = FileGenericIndex::new();
            for comment in root.descendants::<LuaComment>() {
                let mut analyzer = DocAnalyzer::new(
                    db,
                    in_filed_tree.file_id,
                    &mut generic_index,
                    comment,
                    root.syntax().clone(),
                    workspace_id,
                );
                analyze_comment(&mut analyzer);
            }
        }
    }
}

fn analyze_comment(analyzer: &mut DocAnalyzer) -> Option<()> {
    let comment = analyzer.comment.clone();
    for tag in comment.get_doc_tags() {
        tags::analyze_tag(analyzer, tag);
    }

    let owenr = get_owner_id(analyzer, None, false)?;
    let comment_description = preprocess_description(
        &comment.get_description()?.get_description_text(),
        Some(&owenr),
    );
    analyzer.db.get_property_index_mut().add_description(
        analyzer.file_id,
        owenr,
        comment_description,
    );

    Some(())
}

#[derive(Debug)]
pub struct DocAnalyzer<'a> {
    file_id: FileId,
    db: &'a mut DbIndex,
    generic_index: &'a mut FileGenericIndex,
    current_type_id: Option<LuaTypeDeclId>,
    comment: LuaComment,
    root: LuaSyntaxNode,
    is_meta: bool,
    workspace_id: WorkspaceId,
}

impl<'a> DocAnalyzer<'a> {
    pub fn new(
        db: &'a mut DbIndex,
        file_id: FileId,
        generic_index: &'a mut FileGenericIndex,
        comment: LuaComment,
        root: LuaSyntaxNode,
        workspace_id: WorkspaceId,
    ) -> DocAnalyzer<'a> {
        let is_meta = db.get_module_index().is_meta_file(&file_id);
        DocAnalyzer {
            file_id,
            db,
            generic_index,
            current_type_id: None,
            comment,
            root,
            is_meta,
            workspace_id,
        }
    }
}

pub fn preprocess_description(mut description: &str, owner: Option<&LuaSemanticDeclId>) -> String {
    let need_remove_start_char = if let Some(owner) = owner {
        !matches!(owner, LuaSemanticDeclId::Signature(_))
    } else {
        true
    };
    if need_remove_start_char && description.starts_with(['#', '@']) {
        description = description.trim_start_matches(['#', '@']);
    }

    let mut result = String::new();
    let lines = description.lines();
    let mut start_with_one_space = None;
    for mut line in lines {
        let indent_count = line.chars().take_while(|c| c.is_whitespace()).count();
        if indent_count == line.len() {
            // empty line
            result.push('\n');
            continue;
        }

        if start_with_one_space.is_none() {
            start_with_one_space = Some(indent_count == 1);
        }

        if let Some(true) = start_with_one_space {
            let mut chars = line.chars();
            let first_char = chars.next();
            if let Some(c) = first_char
                && c.is_whitespace()
            {
                line = chars.as_str();
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    // trim end
    result.trim_end().to_string()
}
