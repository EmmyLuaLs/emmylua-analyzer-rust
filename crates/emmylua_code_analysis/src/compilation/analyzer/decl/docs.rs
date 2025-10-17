use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaComment, LuaDocTag, LuaDocTagAlias, LuaDocTagAttribute,
    LuaDocTagClass, LuaDocTagEnum, LuaDocTagMeta, LuaDocTagNamespace, LuaDocTagUsing,
    LuaDocTypeFlag,
};
use flagset::FlagSet;
use rowan::TextRange;

use crate::{
    LuaTypeDecl, LuaTypeDeclId,
    db_index::{LuaDeclTypeKind, LuaTypeFlag},
};

use super::DeclAnalyzer;

pub fn analyze_doc_tag_class(analyzer: &mut DeclAnalyzer, class: LuaDocTagClass) -> Option<()> {
    let name_token = class.get_name_token()?;
    let name = name_token.get_name_text().to_string();
    let range = name_token.syntax().text_range();
    let type_flag = get_type_flag_value(analyzer, class.get_type_flag());

    add_type_decl(analyzer, &name, range, LuaDeclTypeKind::Class, type_flag);
    Some(())
}

fn get_type_flag_value(
    analyzer: &mut DeclAnalyzer,
    flag: Option<LuaDocTypeFlag>,
) -> FlagSet<LuaTypeFlag> {
    let mut attr: FlagSet<LuaTypeFlag> = if analyzer.is_meta {
        LuaTypeFlag::Meta.into()
    } else {
        LuaTypeFlag::None.into()
    };

    if let Some(flag) = flag {
        for token in flag.get_attrib_tokens() {
            match token.get_name_text() {
                "partial" => {
                    attr |= LuaTypeFlag::Partial;
                }
                "key" => {
                    attr |= LuaTypeFlag::Key;
                }
                // "global" => {
                //     attr |= LuaTypeAttribute::Global;
                // }
                "exact" => {
                    attr |= LuaTypeFlag::Exact;
                }
                "constructor" => {
                    attr |= LuaTypeFlag::Constructor;
                }
                _ => {}
            }
        }
    }

    attr
}

pub fn analyze_doc_tag_enum(analyzer: &mut DeclAnalyzer, enum_: LuaDocTagEnum) -> Option<()> {
    let name_token = enum_.get_name_token()?;
    let name = name_token.get_name_text().to_string();
    let range = name_token.syntax().text_range();
    let flag = get_type_flag_value(analyzer, enum_.get_type_flag());

    add_type_decl(analyzer, &name, range, LuaDeclTypeKind::Enum, flag);
    Some(())
}

pub fn analyze_doc_tag_alias(analyzer: &mut DeclAnalyzer, alias: LuaDocTagAlias) -> Option<()> {
    let name_token = alias.get_name_token()?;
    let name = name_token.get_name_text().to_string();
    let range = name_token.syntax().text_range();

    add_type_decl(
        analyzer,
        &name,
        range,
        LuaDeclTypeKind::Alias,
        LuaTypeFlag::None.into(),
    );
    Some(())
}

pub fn analyze_doc_tag_attribute(
    analyzer: &mut DeclAnalyzer,
    attribute: LuaDocTagAttribute,
) -> Option<()> {
    let name_token = attribute.get_name_token()?;
    let name = name_token.get_name_text().to_string();
    let range = name_token.syntax().text_range();

    add_type_decl(
        analyzer,
        &name,
        range,
        LuaDeclTypeKind::Attribute,
        LuaTypeFlag::None.into(),
    );
    Some(())
}

pub fn analyze_doc_tag_namespace(
    analyzer: &mut DeclAnalyzer,
    namespace: LuaDocTagNamespace,
) -> Option<()> {
    let name = namespace.get_name_token()?.get_name_text().to_string();

    let file_id = analyzer.get_file_id();
    analyzer
        .db
        .get_type_index_mut()
        .add_file_namespace(file_id, name);

    Some(())
}

pub fn analyze_doc_tag_using(analyzer: &mut DeclAnalyzer, using: LuaDocTagUsing) -> Option<()> {
    let name = using.get_name_token()?.get_name_text().to_string();

    let file_id = analyzer.get_file_id();
    analyzer
        .db
        .get_type_index_mut()
        .add_file_using_namespace(file_id, name);

    Some(())
}

pub fn analyze_doc_tag_meta(analyzer: &mut DeclAnalyzer, tag: LuaDocTagMeta) -> Option<()> {
    let file_id = analyzer.get_file_id();
    analyzer.db.get_module_index_mut().set_meta(file_id);
    analyzer.is_meta = true;

    if let Some(name_token) = tag.get_name_token() {
        let text = name_token.get_name_text();
        // compact luals
        if text == "no-require" || text == "_" {
            analyzer
                .db
                .get_module_index_mut()
                .set_module_visibility(file_id, false);
        } else {
            let workspace_id = analyzer
                .db
                .get_module_index()
                .get_module(file_id)?
                .workspace_id;

            analyzer
                .db
                .get_module_index_mut()
                .add_module_by_module_path(file_id, text.to_string(), workspace_id);
            analyzer.db.get_module_index_mut().set_meta(file_id);
        }
    }

    let comment = tag.get_parent::<LuaComment>()?;
    let version_tag = comment.get_doc_tags().find_map(|tag| {
        if let LuaDocTag::Version(version) = tag {
            Some(version)
        } else {
            None
        }
    })?;

    let mut version_conds = Vec::new();
    for doc_version in version_tag.get_version_list() {
        let version_condition = doc_version.get_version_condition()?;
        version_conds.push(version_condition);
    }

    analyzer
        .db
        .get_module_index_mut()
        .set_module_version_conds(file_id, version_conds);

    Some(())
}

fn add_type_decl(
    analyzer: &mut DeclAnalyzer,
    name: &str,
    range: TextRange,
    kind: LuaDeclTypeKind,
    flag: FlagSet<LuaTypeFlag>,
) {
    let file_id = analyzer.get_file_id();
    let type_index = analyzer.db.get_type_index_mut();

    let basic_name = name;
    let option_namespace = type_index.get_file_namespace(&file_id);
    let full_name = option_namespace
        .map(|ns| format!("{}.{}", ns, basic_name))
        .unwrap_or(basic_name.to_string());
    let id = LuaTypeDeclId::new(&full_name);
    let simple_name = id.get_simple_name();
    type_index.add_type_decl(
        file_id,
        LuaTypeDecl::new(file_id, range, simple_name.to_string(), kind, flag, id),
    );
}
