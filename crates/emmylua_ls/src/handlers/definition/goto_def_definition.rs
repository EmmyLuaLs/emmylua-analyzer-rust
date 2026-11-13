//! # goto_def_definition — pure-semantic declaration lookup
//!
//! `SemanticId::Decl` → declaration name; `Member` → definition site + same-key members (prefix type /
//! table field's table and declaration type); `TypeDef` → all definition positions.
//! The old DbIndex implementation (overload matching / accessor properties / attribute source lookup)
//! is retired; see `migration notes` §M3.

use emmylua_code_analysis::{
    Emmyrc, LuaMemberKey, MemberInfo, SemanticDatabase, SemanticId, SemanticModel, WorkspaceId,
};
use emmylua_parser::{
    LuaAstNode, LuaAstToken, LuaComment, LuaDocAttributeUse, LuaDocDescription, LuaDocFieldKey,
    LuaDocTag, LuaExpr, LuaIndexExpr, LuaLiteralExpr, LuaLiteralToken, LuaLocalStat, LuaNameExpr,
    LuaSyntaxToken, LuaTableExpr, LuaTableField, LuaTokenKind,
};
use lsp_types::{GotoDefinitionResponse, Location};
use rowan::TextSize;

use crate::handlers::common::resolve_alias_origin;
use crate::util::parse_desc;

pub fn goto_def_definition(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    decl: &SemanticId,
    token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    match decl {
        SemanticId::Decl(key) => {
            // Value alias: `local f = t.func` / `local a = b` jump to the real function/member definition.
            if let Some(origin) = resolve_alias_origin(model, decl)
                && origin != *decl
            {
                return goto_def_definition(model, db, &origin, token);
            }
            location_of(db, key.file_id, key.name_range).map(GotoDefinitionResponse::Scalar)
        }
        SemanticId::Member(_) => goto_member_definition(model, db, decl, token),
        SemanticId::TypeDef(key) => {
            let mut locations = Vec::new();
            for def in model.type_defs_in_scope(key.scope, &key.full_name).iter() {
                if let Some(location) = location_of(db, def.file_id, def.name_range) {
                    locations.push(location);
                }
            }
            (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
        }
        _ => None,
    }
}

fn goto_member_definition(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    decl: &SemanticId,
    token: &LuaSyntaxToken,
) -> Option<GotoDefinitionResponse> {
    let mut locations = Vec::new();
    let parent = token.parent();
    let is_usage = parent
        .as_ref()
        .is_some_and(|p| LuaIndexExpr::can_cast(p.kind().into()));

    // 1. Same-key members: usage site → prefix's **declared type**; definition site (table field) → outer declaration's declared type.
    let key = member_key_of(model, decl)?;
    let mut prefix_types = Vec::new();
    if let Some(index_expr) = parent.as_ref().and_then(|p| LuaIndexExpr::cast(p.clone()))
        && let Some(prefix) = index_expr.get_prefix_expr()
    {
        let prefix_decl_type = if let LuaExpr::NameExpr(name_expr) = &prefix {
            model
                .resolve_name(name_expr.get_position())
                .and_then(|decl_id| model.type_of_decl(&decl_id))
        } else {
            None
        };
        match prefix_decl_type {
            // `---@type T` annotation takes priority (mirrors the old `t:func()` behavior of locating only class @field).
            Some(ty) => prefix_types.push(ty),
            None => prefix_types.push(model.type_of_expr(prefix.get_syntax_id())),
        }
    } else if let Some(table_field) = parent.as_ref().and_then(|p| LuaTableField::cast(p.clone()))
        && let Some(table) = table_field.get_parent::<LuaTableExpr>()
    {
        // `---@type T local t = { ... }`: @field member of declared type T.
        if let Some(local_name) = table
            .get_parent::<LuaLocalStat>()
            .and_then(|stat| stat.get_local_name_list().next())
            && let Some(name_token) = local_name.get_name_token()
            && let Some(decl_id) = model.decl_by_offset(name_token.get_position())
            && let Some(ty) = model.type_of_decl(&decl_id)
        {
            prefix_types.push(ty);
        }
    }

    for prefix_type in &prefix_types {
        let infos = model.member_infos_with_key_all(&prefix_type, &key);
        let infos = filter_overload_infos(model, token, &infos);
        for info in infos {
            if let Some(id) = info.id
                && let Some(range) = id.member_key_range()
                && let SemanticId::Member(member_key) = &id
                && let Some(location) = location_of(db, member_key.file_id, range)
                && !locations.contains(&location)
            {
                locations.push(location);
            }
        }
    }
    // Field accessor: fields with `---@[field_accessor]` also jump to get/set methods.
    add_field_accessor_locations(model, db, &prefix_types, &key, &mut locations);
    // Usage site: return on type-level member match (old semantics no longer mix in runtime table members).
    if is_usage && !locations.is_empty() {
        return Some(GotoDefinitionResponse::Array(locations));
    }

    // 2. Member definition site (runtime member / table field definition).
    if let SemanticId::Member(key) = decl
        && let Some(location) = location_of(db, key.file_id, key.key_range)
        && !locations.contains(&location)
    {
        locations.push(location);
    }

    (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
}

fn filter_overload_infos(
    model: &SemanticModel<'_>,
    token: &LuaSyntaxToken,
    infos: &[MemberInfo],
) -> Vec<MemberInfo> {
    let Some(call) = token.parent().and_then(|parent| {
        parent
            .ancestors()
            .find_map(emmylua_parser::LuaCallExpr::cast)
    }) else {
        return infos.to_vec();
    };
    let Some(args) = call.get_args_list() else {
        return infos.to_vec();
    };
    let args = args.get_args().collect::<Vec<_>>();
    if args.is_empty() {
        return infos.to_vec();
    }
    let actual_types = args
        .iter()
        .map(|arg| model.type_of_expr(arg.get_syntax_id()))
        .collect::<Vec<_>>();
    // With actual arguments, only locate "doc @field overload groups": in declaration order, take groups
    // from the first up to the first overload matching the argument types; actual method implementations (`is_method`) are not part of doc-overload display.
    let doc_infos = infos
        .iter()
        .filter(|info| {
            !info.is_method && matches!(info.typ, emmylua_code_analysis::LuaType::DocFunction(_))
        })
        .cloned()
        .collect::<Vec<_>>();
    let match_index = doc_infos.iter().position(|info| {
        let emmylua_code_analysis::LuaType::DocFunction(func) = &info.typ else {
            return false;
        };
        let params = func.get_params();
        params
            .iter()
            .zip(actual_types.iter())
            .all(|((_, param), actual)| {
                param
                    .as_ref()
                    .is_none_or(|param| model.type_check(actual, param))
            })
    });
    match match_index {
        Some(index) => doc_infos.into_iter().take(index + 1).collect(),
        None => doc_infos,
    }
}

/// Member declaration key lookup (passes through directly after normalizing to `LuaMemberKey`).
fn member_key_of(model: &SemanticModel<'_>, decl: &SemanticId) -> Option<LuaMemberKey> {
    let SemanticId::Member(key) = decl else {
        return None;
    };
    let member_model = model.model_for(key.file_id);
    let member = member_model.members()?.iter().find(|m| &m.id == decl)?;
    Some(member.key.clone())
}

fn add_field_accessor_locations(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    prefix_types: &[emmylua_code_analysis::LuaType],
    key: &LuaMemberKey,
    locations: &mut Vec<Location>,
) {
    let LuaMemberKey::Name(field) = key else {
        return;
    };
    let field = field.as_str();
    for prefix_type in prefix_types {
        let type_id = match prefix_type {
            emmylua_code_analysis::LuaType::Ref(id)
            | emmylua_code_analysis::LuaType::Def(id) => Some(id),
            emmylua_code_analysis::LuaType::Generic(generic) => Some(generic.get_base_type_id_ref()),
            _ => None,
        };
        let Some(type_id) = type_id else {
            continue;
        };
        let Some(def) = model.type_def_of(type_id) else {
            continue;
        };
        let Some(spec) = field_accessor_for(model, &def, field) else {
            continue;
        };
        for method_name in spec.accessor_names(field) {
            let method_key = LuaMemberKey::Name(method_name.into());
            for info in model.member_infos_with_key_all(prefix_type, &method_key) {
                if let Some(id) = info.id
                    && let Some(range) = id.member_key_range()
                    && let Some(file_id) = info.file_id
                    && let Some(location) = location_of(db, file_id, range)
                    && !locations.contains(&location)
                {
                    locations.push(location);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum AccessorConvention {
    #[default]
    CamelCase,
    PascalCase,
    SnakeCase,
}

impl AccessorConvention {
    fn from_name(name: &str) -> Self {
        match name {
            "PascalCase" => Self::PascalCase,
            "snake_case" => Self::SnakeCase,
            _ => Self::CamelCase,
        }
    }
}

#[derive(Debug, Default)]
struct FieldAccessorSpec {
    convention: AccessorConvention,
    getter: Option<String>,
    setter: Option<String>,
}

impl FieldAccessorSpec {
    /// Explicit getter/setter names take precedence over the naming convention.
    fn accessor_names(&self, field: &str) -> Vec<String> {
        let mut names = Vec::with_capacity(2);
        names.push(
            self.getter
                .clone()
                .unwrap_or_else(|| accessor_name(self.convention, "get", field)),
        );
        names.push(
            self.setter
                .clone()
                .unwrap_or_else(|| accessor_name(self.convention, "set", field)),
        );
        names
    }
}

fn accessor_name(convention: AccessorConvention, prefix: &str, field: &str) -> String {
    let base = field.trim_start_matches('_');
    let pascal = to_pascal_word(base);
    match convention {
        AccessorConvention::CamelCase => format!("{prefix}{pascal}"),
        AccessorConvention::PascalCase => {
            let mut chars = prefix.chars();
            let Some(first) = chars.next() else {
                return pascal;
            };
            format!(
                "{}{}{}",
                first.to_ascii_uppercase(),
                chars.as_str(),
                pascal
            )
        }
        AccessorConvention::SnakeCase => format!("{prefix}_{}", to_snake_word(base)),
    }
}

fn to_pascal_word(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for part in value.split(['_', '-', '.']) {
        let mut chars = part.chars();
        let Some(first) = chars.next() else {
            continue;
        };
        result.push(first.to_ascii_uppercase());
        result.push_str(chars.as_str());
    }
    result
}

fn to_snake_word(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for (index, ch) in value.chars().enumerate() {
        if ch.is_uppercase() && index != 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

/// Finds the `---@[field_accessor(...)]` attribute that applies to `field`.
///
/// The attribute belongs to the immediately following `---@field` tag; an
/// attribute placed before `---@class` applies to the first field as well.
fn field_accessor_for(
    model: &SemanticModel<'_>,
    def: &emmylua_code_analysis::TypeDef,
    field: &str,
) -> Option<FieldAccessorSpec> {
    let owner_syntax = def.owner_syntax?;
    let owner_range = owner_syntax.get_range();
    let def_model = model.model_for(def.file_id);
    let chunk = def_model.chunk()?;
    for comment in chunk.descendants::<LuaComment>() {
        let owns_definition = comment.get_owner().is_some_and(|owner| {
            let owner_range_of_comment = owner.syntax().text_range();
            owner_range_of_comment.contains_range(owner_range)
                || owner_range_of_comment.start() == owner_range.start()
        });
        if !owns_definition {
            continue;
        }
        let mut pending: Option<FieldAccessorSpec> = None;
        for tag in comment.get_doc_tags() {
            match tag {
                LuaDocTag::AttributeUse(attr_tag) => {
                    if let Some(spec) = attr_tag
                        .get_attribute_uses()
                        .find_map(field_accessor_spec)
                    {
                        pending = Some(spec);
                    }
                }
                LuaDocTag::Field(field_tag) => {
                    let matches = field_tag
                        .get_field_key()
                        .and_then(|key| match key {
                            LuaDocFieldKey::Name(name) => {
                                Some(name.get_name_text() == field)
                            }
                            _ => None,
                        })
                        .unwrap_or(false);
                    if matches {
                        if let Some(spec) = pending {
                            return Some(spec);
                        }
                    } else {
                        pending = None;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn field_accessor_spec(attribute: LuaDocAttributeUse) -> Option<FieldAccessorSpec> {
    if attribute.get_type()?.get_name_text()? != "field_accessor" {
        return None;
    }
    let args: Vec<_> = attribute
        .get_arg_list()
        .map(|list| list.get_args().collect())
        .unwrap_or_default();
    let mut spec = FieldAccessorSpec::default();
    if let Some(convention) = args.first().and_then(string_literal_arg) {
        spec.convention = AccessorConvention::from_name(&convention);
    }
    spec.getter = args.get(1).and_then(string_literal_arg);
    spec.setter = args.get(2).and_then(string_literal_arg);
    Some(spec)
}

fn string_literal_arg(arg: &LuaLiteralExpr) -> Option<String> {
    let LuaLiteralToken::String(token) = arg.get_literal()? else {
        return None;
    };
    Some(token.get_value().to_string())
}

fn location_of(
    db: &SemanticDatabase,
    file_id: emmylua_code_analysis::FileId,
    range: rowan::TextRange,
) -> Option<Location> {
    let document = db.document(file_id)?;
    let uri = document.get_uri()?;
    Some(Location {
        uri,
        range: document.to_lsp_range(range)?,
    })
}

/// Goto definition for doc description references (`:lua:obj:` / `{lua:obj}` / `--- @see`).
pub(super) fn goto_doc_definition(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    token: &LuaSyntaxToken,
    offset: TextSize,
    emmyrc: &Emmyrc,
) -> Option<GotoDefinitionResponse> {
    let is_see = token.kind() == LuaTokenKind::TkDocSeeContent.into();
    let description = if is_see {
        None
    } else {
        Some(
            token
                .parent()
                .and_then(|parent| parent.ancestors().find_map(LuaDocDescription::cast))?,
        )
    };
    let comment = if is_see {
        token
            .parent()
            .and_then(|parent| parent.ancestors().find_map(LuaComment::cast))
    } else {
        description.as_ref()?.get_parent::<LuaComment>()
    };
    let scope_types = comment
        .as_ref()
        .map(|comment| comment_scope_types(model, comment))
        .unwrap_or_default();

    let (names, cursor_index) = if token.kind() == LuaTokenKind::TkDocSeeContent.into() {
        let text = token.text().trim().to_string();
        if text.is_empty() {
            return None;
        }
        let names = text
            .split('.')
            .map(|part| part.to_string())
            .collect::<Vec<_>>();
        let cursor_index = names.len().saturating_sub(1);
        (names, cursor_index)
    } else {
        let document = db.document(model.file_id())?;
        let workspace_id = db
            .workspace_id_of(model.file_id())
            .unwrap_or(WorkspaceId::Main);
        let description = description?;
        let items = parse_desc(
            workspace_id,
            emmyrc,
            document.get_text(),
            description,
            Some(offset.into()),
        );
        let ref_range = items
            .iter()
            .find(|item| {
                item.kind == emmylua_parser_desc::DescItemKind::Ref
                    && item.range.contains_inclusive(offset)
            })?
            .range;
        if ref_range.is_empty() {
            return None;
        }
        let path = emmylua_parser_desc::parse_ref_target(document.get_text(), ref_range, offset)?;
        let names = path
            .iter()
            .map(|(item, _)| item.get_name().map(str::to_string))
            .collect::<Option<Vec<_>>>()?;
        let cursor_index = path
            .iter()
            .position(|(_, range)| range.contains(offset))
            .unwrap_or(names.len().saturating_sub(1));
        (names, cursor_index)
    };

    resolve_doc_path(model, db, &names, cursor_index, &scope_types)
}

fn resolve_doc_path(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    names: &[String],
    cursor_index: usize,
    scope_types: &[emmylua_code_analysis::LuaType],
) -> Option<GotoDefinitionResponse> {
    let full_name = names.join(".");
    if let Some(def) = model.resolve_type_def(&full_name) {
        return Some(GotoDefinitionResponse::Scalar(location_of(
            db,
            def.file_id,
            def.name_range,
        )?));
    }

    if cursor_index > 0 {
        let prefix = names[..cursor_index].join(".");
        if let Some(def) = model.resolve_type_def(&prefix) {
            let ty = model.type_def_ref(&def);
            if let Some(response) =
                member_definition_response(model, db, &[(&ty, &names[cursor_index])])
            {
                return Some(response);
            }
        }
        if model.module_file_of(&prefix).is_some() {
            let module_ty = model.require_module_type(&prefix);
            let infos = model.member_infos_with_key_all(
                &module_ty,
                &LuaMemberKey::Name(names[cursor_index].clone().into()),
            );
            if let Some(response) = infos_to_locations(db, &infos) {
                return Some(response);
            }
        }
    }

    let name = names.get(cursor_index)?;
    if cursor_index == 0 {
        if let Some(def) = model.resolve_type_def(name) {
            return Some(GotoDefinitionResponse::Scalar(location_of(
                db,
                def.file_id,
                def.name_range,
            )?));
        }
        let pairs = scope_types
            .iter()
            .map(|ty| (ty, name.as_str()))
            .collect::<Vec<_>>();
        if let Some(response) = member_definition_response(model, db, &pairs) {
            return Some(response);
        }
        // Bare names in doc references also often point to fields on the current file's types (`@class Z` + `c`).
        if let Some(facts) = model.file_facts()
            && let Some(response) = facts.type_defs.iter().find_map(|def| {
                let ty = model.type_def_ref(def);
                member_definition_response(model, db, &[(&ty, name.as_str())])
            })
        {
            return Some(response);
        }
    }
    None
}

fn member_definition_response(
    model: &SemanticModel<'_>,
    db: &SemanticDatabase,
    pairs: &[(&emmylua_code_analysis::LuaType, &str)],
) -> Option<GotoDefinitionResponse> {
    let mut infos = Vec::new();
    for (ty, name) in pairs {
        let key = LuaMemberKey::Name((*name).to_string().into());
        for info in model.member_infos_with_key_all(ty, &key) {
            if !infos.iter().any(|existing: &MemberInfo| {
                existing.id == info.id && existing.file_id == info.file_id
            }) {
                infos.push(info);
            }
        }
    }
    infos_to_locations(db, &infos)
}

/// Usable types involved in the comment's owner (mirrors the scoped member source used by desc completion).
fn comment_scope_types(
    model: &SemanticModel<'_>,
    comment: &LuaComment,
) -> Vec<emmylua_code_analysis::LuaType> {
    let Some(owner) = comment.get_owner() else {
        return Vec::new();
    };
    let mut types = Vec::new();
    for name in owner.syntax().descendants().filter_map(LuaNameExpr::cast) {
        let ty = model
            .resolve_name(name.get_position())
            .and_then(|decl| model.type_of_decl(&decl))
            .or_else(|| {
                let ty = model.type_of_expr(name.get_syntax_id());
                (!matches!(ty, emmylua_code_analysis::LuaType::Unknown)).then_some(ty)
            });
        if let Some(ty) = ty {
            types.push(ty);
        }
        if let Some(name_text) = name.get_name_text()
            && let Some(def) = model.resolve_type_def(&name_text)
        {
            types.push(model.type_def_ref(&def));
        }
    }
    types
}

fn infos_to_locations(
    db: &SemanticDatabase,
    infos: &[MemberInfo],
) -> Option<GotoDefinitionResponse> {
    let mut locations = Vec::new();
    for info in infos {
        if let Some(id) = &info.id
            && let Some(range) = id.member_key_range()
            && let Some(file_id) = info.file_id
            && let Some(location) = location_of(db, file_id, range)
            && !locations.contains(&location)
        {
            locations.push(location);
        }
    }
    (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
}
