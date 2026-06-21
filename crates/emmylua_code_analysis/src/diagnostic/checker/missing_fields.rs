use hashbrown::{HashMap, HashSet};

use emmylua_parser::{LuaAstNode, LuaTableExpr};

use crate::{DbIndex, DiagnosticCode, LuaMemberOwner, LuaType, SemanticModel};

use super::{Checker, DiagnosticContext, humanize_lint_type};
use itertools::Itertools;

pub struct MissingFieldsChecker;

type RequiredFieldsCache = HashMap<LuaType, HashSet<String>>;
type OptionalFieldTypeCache = HashMap<LuaType, bool>;

impl Checker for MissingFieldsChecker {
    const CODES: &[DiagnosticCode] = &[DiagnosticCode::MissingFields];

    fn check(context: &mut DiagnosticContext, semantic_model: &SemanticModel) {
        let root = semantic_model.get_root().clone();

        let mut required_fields_cache = HashMap::new();
        let mut optional_field_type_cache = HashMap::new();
        for expr in root.descendants::<LuaTableExpr>() {
            check_table_expr(
                context,
                semantic_model,
                &expr,
                &mut required_fields_cache,
                &mut optional_field_type_cache,
            );
        }
    }
}

fn check_table_expr(
    context: &mut DiagnosticContext,
    semantic_model: &SemanticModel,
    expr: &LuaTableExpr,
    required_fields_cache: &mut RequiredFieldsCache,
    optional_field_type_cache: &mut OptionalFieldTypeCache,
) -> Option<()> {
    let db = context.db;

    let table_type = match semantic_model.infer_table_should_be(expr.clone())? {
        LuaType::Union(union) => {
            let mut check_type = None;
            for ty in union.into_vec() {
                match &ty {
                    LuaType::Ref(_)
                    | LuaType::Object(_)
                    | LuaType::Generic(_)
                    | LuaType::Intersection(_) => {
                        if check_type.as_ref().is_some_and(|exists| exists != &ty) {
                            return Some(());
                        }
                        check_type = Some(ty);
                    }
                    LuaType::Table | LuaType::Userdata => {
                        return Some(());
                    }
                    LuaType::TableGeneric(_) => {
                        return Some(());
                    }
                    _ => {}
                }
            }

            let Some(check_type) = check_type else {
                return Some(());
            };
            check_type
        }
        LuaType::TableConst(in_file_range) => {
            let file_id = in_file_range.file_id;
            if file_id == semantic_model.get_file_id() {
                let range = in_file_range.value;
                if expr.get_range() == range {
                    return Some(());
                }
            }

            LuaType::TableConst(in_file_range)
        }

        table_type => table_type,
    };

    let required_fields = get_required_fields(
        db,
        &table_type,
        required_fields_cache,
        optional_field_type_cache,
    )?;
    if required_fields.is_empty() {
        return Some(());
    }

    let fields = expr.get_fields_with_keys();
    if fields.len() > 50 {
        return Some(());
    }

    let current_fields = fields.iter().map(|(_, key)| key.get_path_part()).collect();

    let mut missing_fields = required_fields
        .difference(&current_fields)
        .map(String::as_str)
        .collect::<Vec<_>>();
    if missing_fields.is_empty() {
        return Some(());
    }

    missing_fields.sort_unstable();
    let missing_fields = missing_fields
        .into_iter()
        .map(|field| format!("`{}`", field))
        .join(", ");
    context.add_diagnostic(
        DiagnosticCode::MissingFields,
        expr.get_range(),
        t!(
            "Missing required fields in type `%{typ}`: %{fields}",
            typ = humanize_lint_type(db, &table_type),
            fields = missing_fields
        )
        .to_string(),
        None,
    );

    Some(())
}

fn get_required_fields<'a>(
    db: &DbIndex,
    table_type: &LuaType,
    required_fields_cache: &'a mut RequiredFieldsCache,
    optional_field_type_cache: &mut OptionalFieldTypeCache,
) -> Option<&'a HashSet<String>> {
    match table_type {
        LuaType::Ref(type_decl_id) => Some(
            required_fields_cache
                .entry(table_type.clone())
                .or_insert_with(|| {
                    let types = type_decl_id.collect_super_types_with_self(db, table_type.clone());
                    collect_required_fields(db, &types, optional_field_type_cache)
                }),
        ),
        LuaType::Generic(generic_type) => {
            let type_decl_id = generic_type.get_base_type_id();
            Some(
                required_fields_cache
                    .entry(table_type.clone())
                    .or_insert_with(|| {
                        let types =
                            type_decl_id.collect_super_types_with_self(db, table_type.clone());
                        collect_required_fields(db, &types, optional_field_type_cache)
                    }),
            )
        }
        LuaType::Object(_) => Some(
            required_fields_cache
                .entry(table_type.clone())
                .or_insert_with(|| {
                    collect_required_fields(
                        db,
                        std::slice::from_ref(table_type),
                        optional_field_type_cache,
                    )
                }),
        ),
        LuaType::Intersection(intersections) => Some(
            required_fields_cache
                .entry(table_type.clone())
                .or_insert_with(|| {
                    let mut computed_fields = HashSet::new();
                    for intersection_component in intersections.get_types() {
                        computed_fields.extend(collect_required_fields(
                            db,
                            std::slice::from_ref(intersection_component),
                            optional_field_type_cache,
                        ));
                    }
                    computed_fields
                }),
        ),
        _ => None,
    }
}

fn collect_required_fields(
    db: &DbIndex,
    // types 应为广度优先, 子类型会先于父类型被遍历, 而子类型的优先级高于父类型
    types: &[LuaType],
    optional_field_type_cache: &mut OptionalFieldTypeCache,
) -> HashSet<String> {
    let member_index = db.get_member_index();
    let type_index = db.get_type_index();
    let mut required_fields: HashSet<String> = HashSet::new();

    let mut optional_type = HashSet::new();
    for super_type in types {
        // 处理 ---@class test: { a: number }
        if let LuaType::Object(object_type) = super_type {
            let fields = object_type.get_fields();
            for (key, decl_type) in fields {
                let name = key.to_path();
                record_required_fields(
                    &mut required_fields,
                    &mut optional_type,
                    db,
                    optional_field_type_cache,
                    name,
                    decl_type,
                );
            }
            continue;
        }

        let type_decl_id = match super_type {
            LuaType::Ref(type_decl_id) => type_decl_id.clone(),
            LuaType::Generic(generic_type) => generic_type.get_base_type_id(),
            _ => continue,
        };

        let Some(members) = member_index.get_members(&LuaMemberOwner::Type(type_decl_id)) else {
            continue;
        };

        for member in members {
            let name = member.get_key().to_path();
            let decl_type = type_index
                .get_type_cache(&member.get_id().into())
                .map(|type_cache| type_cache.as_type())
                .unwrap_or(&LuaType::Unknown);
            record_required_fields(
                &mut required_fields,
                &mut optional_type,
                db,
                optional_field_type_cache,
                name,
                decl_type,
            );
        }
    }

    required_fields
}

fn record_required_fields(
    required_fields: &mut HashSet<String>,
    optional_type: &mut HashSet<String>,
    db: &DbIndex,
    optional_field_type_cache: &mut OptionalFieldTypeCache,
    name: String,
    decl_type: &LuaType,
) {
    if name.is_empty() {
        return;
    }

    if field_type_is_optional(db, optional_field_type_cache, decl_type) {
        optional_type.insert(name);
        return;
    }

    if !optional_type.contains(&name) {
        required_fields.insert(name);
    }
}

fn field_type_is_optional(
    db: &DbIndex,
    optional_field_type_cache: &mut OptionalFieldTypeCache,
    decl_type: &LuaType,
) -> bool {
    if let Some(is_optional) = optional_field_type_cache.get(decl_type) {
        return *is_optional;
    }

    let mut stack = vec![decl_type.clone()];
    let mut visited = HashSet::new();
    let mut is_optional = false;
    while let Some(typ) = stack.pop() {
        if !visited.insert(typ.clone()) {
            continue;
        }

        match typ {
            LuaType::Any | LuaType::Nil => {
                is_optional = true;
                break;
            }
            LuaType::Ref(type_decl_id) => {
                if let Some(type_decl) = db.get_type_index().get_type_decl(&type_decl_id)
                    && let Some(alias_origin) = type_decl.get_alias_origin(db, None)
                {
                    stack.push(alias_origin);
                }
            }
            LuaType::Union(union) => {
                stack.extend(union.into_vec());
            }
            LuaType::MultiLineUnion(multi_line_union) => {
                for (union_member, _) in multi_line_union.get_unions() {
                    stack.push(union_member.clone());
                }
            }
            _ => {}
        }
    }

    optional_field_type_cache.insert(decl_type.clone(), is_optional);
    is_optional
}
