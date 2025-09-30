use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaComment, LuaDocTag, LuaDocTagAlias, LuaDocTagAttribute,
    LuaDocTagClass, LuaDocTagEnum, LuaDocTagMeta, LuaDocTagNamespace, LuaDocTagUsing,
    LuaDocTypeFlag,
};
use flagset::FlagSet;
use rowan::TextRange;

use crate::{
    LuaTypeDecl, LuaTypeDeclId,
    db_index::{LuaDeclTypeKind, LuaTypeAttribute},
};

use super::DeclAnalyzer;

pub fn analyze_doc_tag_class(analyzer: &mut DeclAnalyzer, class: LuaDocTagClass) -> Option<()> {
    let name_token = class.get_name_token()?;
    let name = name_token.get_name_text().to_string();
    let range = name_token.syntax().text_range();
    let attrib = get_attrib_value(analyzer, class.get_attrib());

    add_type_decl(analyzer, &name, range, LuaDeclTypeKind::Class, attrib);
    Some(())
}

fn get_attrib_value(
    analyzer: &mut DeclAnalyzer,
    attrib: Option<LuaDocTypeFlag>,
) -> FlagSet<LuaTypeAttribute> {
    let mut attr: FlagSet<LuaTypeAttribute> = if analyzer.is_meta {
        LuaTypeAttribute::Meta.into()
    } else {
        LuaTypeAttribute::None.into()
    };

    if let Some(attrib) = attrib {
        for token in attrib.get_attrib_tokens() {
            match token.get_name_text() {
                "partial" => {
                    attr |= LuaTypeAttribute::Partial;
                }
                "key" => {
                    attr |= LuaTypeAttribute::Key;
                }
                // "global" => {
                //     attr |= LuaTypeAttribute::Global;
                // }
                "exact" => {
                    attr |= LuaTypeAttribute::Exact;
                }
                "constructor" => {
                    attr |= LuaTypeAttribute::Constructor;
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
    let attrib = get_attrib_value(analyzer, enum_.get_attrib());

    add_type_decl(analyzer, &name, range, LuaDeclTypeKind::Enum, attrib);
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
        LuaTypeAttribute::None.into(),
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

    // LuaTypeAttribute 与 LuaDocTagAttribute 完全是两个不同的概念.
    // LuaTypeAttribute 描述类型的属性, LuaDocTagAttribute 类似于 C# 的特性, 附加了更多的信息.
    add_type_decl(
        analyzer,
        &name,
        range,
        LuaDeclTypeKind::Attribute,
        LuaTypeAttribute::None.into(),
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
    attrib: FlagSet<LuaTypeAttribute>,
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
        LuaTypeDecl::new(file_id, range, simple_name.to_string(), kind, attrib, id),
    );
}
