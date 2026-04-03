use emmylua_code_analysis::{
    DbIndex, DocSyntax, Emmyrc, FileId, LuaMemberId, LuaMemberKey, LuaType, LuaTypeDeclId,
    SemanticInfo, WorkspaceId, get_member_map,
};
use emmylua_parser::{
    LuaAst, LuaAstNode, LuaComment, LuaDocDescription, LuaDocTag, LuaLocalName, LuaSyntaxToken,
    LuaVarExpr,
};
use emmylua_parser_desc::{
    DescItem, DescItemKind, DescParserType, LuaDescRefPathItem, parse_ref_target,
};
use itertools::Itertools;
use rowan::{TextRange, TextSize};
use std::collections::HashSet;

pub fn parse_desc(
    workspace_id: WorkspaceId,
    emmyrc: &Emmyrc,
    text: &str,
    desc: LuaDocDescription,
    offset: Option<usize>,
) -> Vec<DescItem> {
    let parser_kind = if workspace_id == WorkspaceId::STD {
        DescParserType::Md
    } else {
        match emmyrc.doc.syntax {
            DocSyntax::None => DescParserType::None,
            DocSyntax::Md => DescParserType::Md,
            DocSyntax::Myst => DescParserType::MySt {
                primary_domain: emmyrc.doc.rst_primary_domain.clone(),
            },
            DocSyntax::Rst => DescParserType::Rst {
                primary_domain: emmyrc.doc.rst_primary_domain.clone(),
                default_role: emmyrc.doc.rst_default_role.clone(),
            },
        }
    };

    emmylua_parser_desc::parse(parser_kind, text, desc, offset)
}

pub fn find_ref_at(
    workspace_id: WorkspaceId,
    emmyrc: &Emmyrc,
    text: &str,
    desc: LuaDocDescription,
    offset: TextSize,
) -> Option<Vec<(LuaDescRefPathItem, TextRange)>> {
    let items = parse_desc(workspace_id, emmyrc, text, desc, Some(offset.into()));

    for item in items {
        if matches!(item.kind, DescItemKind::Ref | DescItemKind::JavadocLink) {
            if !item.range.contains_inclusive(offset) {
                continue;
            }

            return parse_ref_target(text, item.range, offset);
        }
    }

    None
}

pub fn resolve_ref_single(
    db: &DbIndex,
    file_id: FileId,
    path: &[(LuaDescRefPathItem, TextRange)],
    desc: &LuaSyntaxToken,
) -> Option<SemanticInfo> {
    let results = resolve_ref(db, file_id, path, desc);
    for (i, result) in results.iter().enumerate() {
        if result.semantic_decl.is_some() {
            return Some(results[i].clone());
        }
    }

    results.into_iter().next()
}

pub fn resolve_ref(
    db: &DbIndex,
    file_id: FileId,
    path: &[(LuaDescRefPathItem, TextRange)],
    desc: &LuaSyntaxToken,
) -> Vec<SemanticInfo> {
    let mut result = Vec::new();

    // Try resolving in comment's owner. I.e. documentation for a class
    // can refer to class members without prefixing them by class name.
    if let Some(scope) = find_comment_scope(db, file_id, desc) {
        let scopes = vec![SemanticInfo {
            typ: LuaType::Ref(scope.clone()),
            semantic_decl: Some(scope.into()),
        }];
        if let Some(found_refs) = find_members(db, scopes, path) {
            result.extend(found_refs);
        }
    }

    // Find in namespaces and modules.

    // We need to deduplicate types found in namespace and module.
    //
    // For completion path `foo.bar`, we look up types in namespace
    // `foo.bar`, as well as items exported from module `foo.bar`.
    // This might result in duplicates when a module exports a definition
    // of a type that's defined in a corresponding namespace.
    //
    // For example, consider this module:
    //
    // ```
    // local mod = {}
    // --- @class Foo
    // mod.Foo = {}
    // return mod
    // ```
    //
    // Our search will find `@class` declaration `LuaType::Ref("mod.Foo")`,
    // and also `mod.Foo` definition `LuaType::Def("mod.Foo")`,
    // which we will ignore.
    let mut seen_types = HashSet::new();

    let last_name_index = path.iter().take_while(|(item, _)| item.is_name()).count();
    for i in (1..=last_name_index).rev() {
        let name = path[..i]
            .iter()
            .filter_map(|(item, _)| item.get_name())
            .join(".");

        if let Some(found) =
            db.get_type_index()
                .find_type_decl(file_id, &name, db.resolve_workspace_id(file_id))
        {
            let scopes = vec![SemanticInfo {
                typ: LuaType::Ref(found.get_id()),
                semantic_decl: Some(found.get_id().into()),
            }];
            if let Some(found_refs) = find_members(db, scopes, &path[i..]) {
                seen_types.extend(found_refs.iter().filter_map(|item| match &item.typ {
                    LuaType::Ref(id) => Some(LuaType::Def(id.clone())),
                    _ => None,
                }));
                result.extend(found_refs);
            }
        }
        if let Some(found) = db.get_module_index().find_module(&name) {
            let scopes = vec![SemanticInfo {
                typ: found.export_type.clone().unwrap_or(LuaType::Nil),
                semantic_decl: found.semantic_id.clone(),
            }];
            if let Some(found_refs) = find_members(db, scopes, &path[i..]) {
                result.extend(
                    found_refs
                        .into_iter()
                        .filter(|item| !seen_types.contains(&item.typ)),
                );
            }
        }
    }

    // Find in current module.
    if let Some(module) = db.get_module_index().get_module(file_id) {
        let scopes = vec![SemanticInfo {
            typ: module.export_type.clone().unwrap_or(LuaType::Nil),
            semantic_decl: module.semantic_id.clone(),
        }];
        if let Some(found_refs) = find_members(db, scopes, path) {
            result.extend(
                found_refs
                    .into_iter()
                    .filter(|item| !seen_types.contains(&item.typ)),
            );
        }
    }

    // Find in globals.
    if let Some((LuaDescRefPathItem::Name(name), _)) = path.first()
        && let Some(globals) = db.get_global_index().get_global_decl_ids(name)
    {
        let scopes = globals
            .iter()
            .filter_map(|&global| {
                Some(SemanticInfo {
                    typ: db
                        .get_type_index()
                        .get_type_cache(&global.into())?
                        .as_type()
                        .clone(),
                    semantic_decl: Some(global.into()),
                })
            })
            .collect();
        if let Some(found_refs) = find_members(db, scopes, &path[1..]) {
            result.extend(found_refs);
        }
    }

    result
}

pub fn find_comment_scope(
    db: &DbIndex,
    file_id: FileId,
    desc: &LuaSyntaxToken,
) -> Option<LuaTypeDeclId> {
    let parent = LuaComment::cast(desc.parent()?.parent()?)?;

    // 1. Try doc tags.
    for tag in parent.get_doc_tags() {
        let name_tag = match tag {
            LuaDocTag::Class(def) => def.get_name_token()?,
            LuaDocTag::Enum(def) => def.get_name_token()?,
            LuaDocTag::Alias(def) => def.get_name_token()?,
            _ => continue,
        };

        return Some(
            db.get_type_index()
                .find_type_decl(
                    file_id,
                    name_tag.get_name_text(),
                    db.resolve_workspace_id(file_id),
                )?
                .get_id(),
        );
    }

    // Try comment owner.
    let owner = parent.get_owner()?;
    let owner_syntax_id = match owner {
        LuaAst::LuaAssignStat(stat) => {
            let first_var = stat.child::<LuaVarExpr>()?;
            match first_var {
                LuaVarExpr::NameExpr(name_expr) => name_expr.get_syntax_id(),
                LuaVarExpr::IndexExpr(index_expr) => index_expr.get_syntax_id(),
            }
        }
        LuaAst::LuaLocalStat(stat) => stat.child::<LuaLocalName>()?.get_syntax_id(),
        LuaAst::LuaTableField(stat) => stat.get_syntax_id(),
        LuaAst::LuaFuncStat(stat) => stat.get_func_name()?.get_syntax_id(),
        _ => return None,
    };
    // Comment owner is a member of some class/type, try to find it.
    let member_id = LuaMemberId::new(owner_syntax_id, file_id);
    db.get_member_index()
        .get_current_owner(&member_id)?
        .get_type_id()
        .cloned()
}

fn find_members(
    db: &DbIndex,
    mut scopes: Vec<SemanticInfo>,
    path: &[(LuaDescRefPathItem, TextRange)],
) -> Option<Vec<SemanticInfo>> {
    for (item, _) in path {
        let member_key = match item {
            LuaDescRefPathItem::Name(name) => LuaMemberKey::Name(name.into()),
            LuaDescRefPathItem::Number(num) => LuaMemberKey::Integer(*num),
            LuaDescRefPathItem::Type(_) => {
                // XXX: supporting complex types requires additional consideration,
                //      skip it for now.
                return None;
            }
        };

        let mut new_scopes = Vec::new();

        for scope in scopes {
            let members = get_member_map(db, &scope.typ);
            if let Some(found_members) = members
                .as_ref()
                .and_then(|members| members.get(&member_key))
            {
                new_scopes.extend(found_members.iter().map(|member| SemanticInfo {
                    typ: member.typ.clone(),
                    semantic_decl: member.property_owner_id.clone(),
                }))
            }
        }

        if new_scopes.is_empty() {
            return None;
        }

        scopes = new_scopes;
    }

    Some(scopes)
}
