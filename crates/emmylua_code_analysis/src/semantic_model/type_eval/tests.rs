use super::*;
use crate::semantic_model::type_check::check_type;
use crate::{LuaTypeDeclId, VirtualWorkspace};

#[test]
fn recursive_mapped_members_do_not_depend_on_query_order() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@alias Copy<T> {[K in keyof T]: T[K]}
        ---@alias Node<T> {value: T, copy: Copy<Node<T>>}
        "#,
    );
    let ty = ws.ty("Node<string>");
    for expand_first in [true, false] {
        let model = ws.analysis.semantic_model(file);
        if expand_first {
            expand_alias_generic(&model, &ty);
        }
        for _ in 0..2 {
            let members = model.member_infos(&ty);
            assert_eq!(members.len(), 2);
            let value = members
                .iter()
                .find(|info| info.key == LuaMemberKey::Name("value".into()));
            assert_eq!(value.map(|info| &info.typ), Some(&LuaType::String));
            expand_alias_generic(&model, &ty);
        }
    }
}

#[test]
fn unresolved_conditional_comparisons_stay_deferred() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Cycle Other\n---@alias Other Cycle");
    let unresolved = [
        ws.ty("Cycle extends string and boolean or integer"),
        ws.ty("string extends (Cycle | integer) and boolean or integer"),
        ws.ty("Cycle extends {value: infer T} and T or integer"),
    ];
    let accepted = ws.ty("string extends string and boolean or integer");
    let rejected = ws.ty("integer extends string and boolean or integer");
    let known_union_branch = ws.ty("string extends (Cycle | string) and boolean or integer");
    let model = ws.analysis.semantic_model(file);
    model.type_check(
        &LuaType::Ref(LuaTypeDeclId::global("Cycle")),
        &LuaType::String,
    );
    for unresolved in unresolved {
        assert!(matches!(&unresolved, LuaType::Conditional(_)));
        assert_eq!(eval_conditionals(&model, &unresolved), unresolved);
    }
    assert_eq!(eval_conditionals(&model, &accepted), LuaType::Boolean);
    assert_eq!(eval_conditionals(&model, &rejected), LuaType::Integer);
    assert_eq!(
        eval_conditionals(&model, &known_union_branch),
        LuaType::Boolean
    );
}

#[test]
fn incomplete_alternatives_do_not_hide_complete_comparison_proofs() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Cycle Other\n---@alias Other Cycle");
    for source in ["Cycle & true", "true & Cycle"] {
        let ty = ws.ty(source);
        let tuple = ws.ty(&format!("[{source}]"));
        let tuple_target = ws.ty("[boolean]");
        let conditional = ws.ty(&format!("({source}) extends boolean and string or integer"));
        let inferred = ws.ty(&format!(
            "[{source}, string] extends [boolean, infer T] and T or integer"
        ));
        let unresolved = ws.ty(&format!("({source}) extends string and boolean or integer"));
        let model = ws.analysis.semantic_model(file);
        for _ in 0..2 {
            assert_eq!(eval_conditionals(&model, &inferred), LuaType::String);
            assert!(model.type_check(&ty, &LuaType::Boolean), "{source}");
            assert!(model.type_check_subtype(&ty, &LuaType::Boolean), "{source}");
            assert!(model.type_check(&tuple, &tuple_target), "{source}");
            assert_eq!(eval_conditionals(&model, &conditional), LuaType::String);
            assert_eq!(eval_conditionals(&model, &unresolved), unresolved);
        }
    }
}

#[test]
fn definite_mismatches_decide_required_comparisons() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Cycle Other\n---@alias Other Cycle");
    for (source, target) in [
        ("[Cycle, integer]", "[string, string]"),
        ("[integer, Cycle]", "[string, string]"),
        ("table<Cycle, integer>", "table<string, string>"),
        ("table<integer, Cycle>", "table<string, string>"),
        ("integer[]", "[Cycle, string]"),
        (
            "(fun(a: Cycle, b: integer): string)",
            "(fun(a: string, b: string): string)",
        ),
        ("(fun(a: Cycle): integer)", "(fun(a: string): string)"),
        ("[Cycle, integer, boolean]", "[string, string, infer T]"),
        ("[integer, Cycle, boolean]", "[string, string, infer T]"),
        (
            "(fun(a: Cycle, b: integer): boolean)",
            "(fun(a: string, b: string): infer T)",
        ),
    ] {
        let conditional = ws.ty(&format!("{source} extends {target} and boolean or number"));
        let model = ws.analysis.semantic_model(file);
        for _ in 0..2 {
            assert_eq!(
                eval_conditionals(&model, &conditional),
                LuaType::Number,
                "{source}"
            );
        }
    }
    for condition in [
        "[Cycle, string] extends [string, string] and boolean or number",
        "table<string, Cycle> extends table<string, string> and boolean or number",
        "[Cycle, string] extends [boolean, infer T] and T or integer",
        "(fun(a: Cycle): string) extends (fun(a: string): string) and boolean or number",
    ] {
        let conditional = ws.ty(condition);
        let model = ws.analysis.semantic_model(file);
        assert_eq!(eval_conditionals(&model, &conditional), conditional);
    }
}

#[test]
fn unfinished_union_members_are_not_matches_or_mismatches() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Cycle Other\n---@alias Other Cycle");
    for target in ["Cycle | true", "true | Cycle"] {
        let ty = ws.ty(target);
        let conditional = ws.ty(&format!("[true] extends [{target}] and string or integer"));
        let unresolved = ws.ty(&format!("[false] extends [{target}] and string or integer"));
        let model = ws.analysis.semantic_model(file);
        for _ in 0..2 {
            assert!(model.type_check(&LuaType::BooleanConst(true), &ty));
            assert!(!model.type_check(&LuaType::BooleanConst(false), &ty));
            assert_eq!(eval_conditionals(&model, &conditional), LuaType::String);
            assert_eq!(eval_conditionals(&model, &unresolved), unresolved);
        }
    }
}

#[test]
#[ntest::timeout(5000)]
fn recursive_alternatives_keep_only_proven_answers() {
    for operator in ["|", "&"] {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(&format!(
            "---@alias Branch Left {operator} Right\n---@alias Left Branch\n---@alias Right Branch\n---@alias Productive Loop {operator} integer\n---@alias Loop Productive"
        ));
        let ty = ws.ty("Branch");
        let conditional = ws.ty("Branch extends integer and string or boolean");
        let productive = ws.ty("Productive extends integer and string or boolean");
        let model = ws.analysis.semantic_model(file);
        assert!(!model.type_check(&ty, &LuaType::Integer));
        assert_eq!(eval_conditionals(&model, &conditional), conditional);
        assert_eq!(eval_conditionals(&model, &productive), LuaType::String);
    }
}

#[test]
fn growing_alias_definitions_are_explicitly_unsupported() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@alias Grow<T> T | Grow<T[]>
        ---@alias Left<T> Right<T[]>
        ---@alias Right<T> Left<T>
        ---@alias Productive<T> Grow<T> | integer
        ---@alias InferGrow<T> T extends infer U and InferGrow<U[]> or never
        ---@alias ObjectGrow<T> T extends {value: infer U} and ObjectGrow<{value: {value: U}}> or never
        ---@class Box<T>
        ---@field value T
        ---@alias BoxGrow<T> T extends Box<infer U> and BoxGrow<Box<Box<U>>> or never
        ---@alias ReturnGrow<T> T extends (fun(): infer U) and ReturnGrow<fun(): (fun(): U)> or never
        ---@alias RestGrow<T> T extends (fun(...: infer U)) and RestGrow<fun(value: U)> or never
        "#,
    );
    for name in [
        "Grow<string>",
        "Left<string>",
        "Right<string>",
        "InferGrow<string>",
        "ObjectGrow<{value: string}>",
        "BoxGrow<Box<string>>",
        "ReturnGrow<fun(): string>",
        "RestGrow<fun(value: string)>",
    ] {
        let source = ws.ty(name);
        let conditional = ws.ty(&format!("{name} extends integer and boolean or string"));
        let model = ws.analysis.semantic_model(file);
        assert!(
            matches!(
                check_type(&model, &source, &LuaType::Integer),
                Err(TypeCheckFailReason::UnsupportedAlias(_))
            ),
            "{name}"
        );
        assert_eq!(eval_conditionals(&model, &conditional), conditional);
        assert_eq!(expand_alias_generic(&model, &source), source);
    }
    let known = ws.ty("Productive<string> extends integer and boolean or string");
    let model = ws.analysis.semantic_model(file);
    assert_eq!(eval_conditionals(&model, &known), LuaType::Boolean);
}

#[test]
fn recursive_alias_arguments_can_change_within_a_finite_set() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@alias Change<T> T | Change<integer>
        ---@alias Switch<A, B> {value: A, next: Switch<B, A>}
        ---@alias Tail {value: integer}
        "#,
    );
    let changed = ws.ty("Change<string>");
    let switch = ws.ty("Switch<string, integer>");
    let accepted = ws.ty("{value: string, next: Tail}");
    let rejected = ws.ty("{value: string, next: {value: boolean}}");
    let model = ws.analysis.semantic_model(file);
    assert!(model.type_check(&changed, &LuaType::Integer));
    assert!(model.type_check_subtype(&switch, &accepted));
    assert!(!model.type_check_subtype(&switch, &rejected));
}

#[test]
fn inferred_components_can_consume_recursive_alias_arguments() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@alias Flatten<T> T extends (infer U)[] and Flatten<U> or T
        ---@alias Unwrap<T> T extends {value: infer U} and Unwrap<U> or T
        ---@class Box<T>
        ---@field value T
        ---@alias Unbox<T> T extends Box<infer U> and Unbox<U> or T
        ---@alias Returns<T> T extends (fun(): infer U) and Returns<U> or T
        ---@alias Argument<T> T extends (fun(value: infer U)) and Argument<U> or T
        ---@alias FirstArgument<T> T extends (fun(...: [infer U])) and FirstArgument<U> or T
        ---@class Peel<T>
        ---@field value Flatten<T>
        ---@alias ProjectedFlatten<T> RawGet<Peel<T>, 'value'>
        ---@class RecursiveBox
        ---@field value RecursiveBox[]
        ---@alias Wrapped<T> T | Reset<T[]>
        ---@alias Reset<T> Wrapped<integer>
        "#,
    );
    for (source, expected) in [
        ("Flatten<string[][][]>", "string"),
        ("ProjectedFlatten<string[][][]>", "string"),
        ("Unwrap<{value: {value: string}}>", "string"),
        ("Unwrap<RecursiveBox>", "RecursiveBox[]"),
        ("Unbox<Box<Box<string>>>", "string"),
        ("Returns<fun(): (fun(): string)>", "string"),
        ("Argument<fun(value: (fun(value: string)))>", "string"),
        ("FirstArgument<fun(value: (fun(value: string)))>", "string"),
    ] {
        let source = ws.ty(source);
        let expected = ws.ty(expected);
        let model = ws.analysis.semantic_model(file);
        assert_eq!(
            expand_alias_generic(&model, &source),
            expected,
            "{source:?}"
        );
    }
    let wrapped = ws.ty("Wrapped<string>");
    let model = ws.analysis.semantic_model(file);
    assert!(model.type_check(&wrapped, &LuaType::Integer));
}

#[test]
fn recursive_json_aliases_keep_finite_nested_values() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@alias Json nil | boolean | number | string | Json[] | table<string, Json>
        ---@alias JsonOf<T> T | JsonOf<T>[] | table<string, JsonOf<T>>
        "#,
    );
    for alias in ["Json", "JsonOf<nil | boolean | number | string>"] {
        let target = ws.ty(alias);
        for source in [
            "string",
            "boolean",
            "number[][]",
            "table<string, string[]>",
            "fun()",
            "thread",
        ] {
            let source_type = ws.ty(source);
            let model = ws.analysis.semantic_model(file);
            let result = crate::semantic_model::type_check::check_assign_type_detail(
                &model,
                &source_type,
                &target,
            );
            if matches!(source, "fun()" | "thread") {
                assert!(
                    result.is_err_and(|err| err.is_type_not_match()),
                    "{alias}: {source}"
                );
            } else {
                assert!(result.is_ok(), "{alias}: {source}");
            }
        }
    }
}

#[test]
fn finite_comparisons_have_no_depth_cutoff() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Nested string | Nested[]");
    let target = ws.ty("Nested");
    for (leaf, accepted) in [("string", true), ("(fun())", false)] {
        let source = ws.ty(&format!("{leaf}{}", "[]".repeat(256)));
        let model = ws.analysis.semantic_model(file);
        let result =
            crate::semantic_model::type_check::check_assign_type_detail(&model, &source, &target);
        assert_eq!(result.is_ok(), accepted, "{leaf}");
        if !accepted {
            assert!(result.is_err_and(|err| err.is_type_not_match()));
        }
    }
}

#[test]
fn function_parameters_keep_independent_alias_paths() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Identity<T> T\n---@alias Num integer");
    for (source, target, expected) in [
        ("Identity<fun(x: integer)>", "fun(x: Identity<Num>)", true),
        (
            "Identity<fun(x: integer)>",
            "fun(x: Identity<string>)",
            false,
        ),
        ("Identity<fun(...: integer)>", "fun(x: Identity<Num>)", true),
        (
            "Identity<fun(...: integer)>",
            "fun(x: Identity<string>)",
            false,
        ),
    ] {
        let source_type = ws.ty(source);
        let target_type = ws.ty(target);
        let model = ws.analysis.semantic_model(file);
        assert_eq!(model.type_check(&source_type, &target_type), expected);
        assert_eq!(
            model.type_check_subtype(&source_type, &target_type),
            expected
        );
        assert_eq!(
            crate::semantic_model::type_check::check_assign_type_detail(
                &model,
                &source_type,
                &target_type
            )
            .is_ok(),
            expected
        );
    }
}

#[test]
fn recursive_alias_comparisons_can_descend_into_finite_objects() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Link<T> {value: T, next: Link<T>}");
    let source = ws.ty("Link<string>");
    let accepted = ws.ty("{value: string, next: {value: string}}");
    let rejected = ws.ty("{value: string, next: {value: integer}}");
    let model = ws.analysis.semantic_model(file);
    assert!(model.type_check_subtype(&source, &accepted));
    assert!(!model.type_check_subtype(&source, &rejected));
}

#[test]
fn indexed_aliases_can_use_the_same_alias_for_base_and_key() {
    let mut ws = VirtualWorkspace::new_with_init_std_lib();
    let file = ws.def("---@alias Key 'len'");
    let types = [
        ws.ty("RawGet<Key, Key>"),
        ws.ty("RawGet<Key, 'len'>"),
        ws.ty("RawGet<'len', Key>"),
    ];
    let model = ws.analysis.semantic_model(file);
    let expected = model
        .member_type(&LuaType::String, &LuaMemberKey::Name("len".into()))
        .expect("string.len must be defined");
    for ty in types {
        assert_eq!(expand_alias_generic(&model, &ty), expected);
    }
}

#[test]
fn recursive_structures_keep_definite_compatibility_answers() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@alias Leaf<T> {value: T}
        ---@alias Tree<T> Leaf<T> | Tree<T>[]
        ---@alias Copy<T> {[K in keyof T]: T[K]}
        ---@alias Node<T> {value: T, copy: Copy<Node<T>>}
        "#,
    );
    let types = [ws.ty("Tree<string>"), ws.ty("Node<string>")];
    let conditional = ws.ty("Node<string> extends table and boolean or integer");
    let model = ws.analysis.semantic_model(file);
    for ty in types {
        for _ in 0..2 {
            assert!(model.type_check(&ty, &LuaType::Table));
            assert!(!model.type_check(&ty, &LuaType::Integer));
        }
    }
    assert_eq!(eval_conditionals(&model, &conditional), LuaType::Boolean);
}

#[test]
fn shrinking_aliases_and_independent_fields_resolve() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Identity<T> T\n---@alias Pair<T> {left: T, right: T}");
    let ty = ws.ty("Pair<Identity<Identity<string>>>");
    let expected = ws.ty("{left: string, right: string}");
    let model = ws.analysis.semantic_model(file);
    assert_eq!(expand_alias_generic(&model, &ty), expected);
}

#[test]
fn conditional_aliases_resolve_shrinking_arguments_and_union_members() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def("---@alias Strings<T> T extends string and T or never");
    let ty = ws.ty("Strings<Strings<string> | integer>");
    let model = ws.analysis.semantic_model(file);
    assert_eq!(expand_alias_generic(&model, &ty), LuaType::String);
}

#[test]
fn alias_cycles_stop_and_leave_other_queries_usable() {
    for (definitions, name) in [
        ("---@alias Cycle Cycle", "Cycle"),
        ("---@alias Cycle Other\n---@alias Other Cycle", "Cycle"),
        ("---@alias Cycle<T> Cycle<T>", "Cycle<string>"),
        (
            "---@alias Cycle<T> Other<T>\n---@alias Other<T> Cycle<T>",
            "Cycle<string>",
        ),
        ("---@alias Cycle<T> Cycle<T[]>", "Cycle<string>"),
        (
            "---@alias Cycle<T> Other<Cycle<T[]>>\n---@alias Other<T> T extends string and T or T[]",
            "Cycle<string>",
        ),
    ] {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(&format!("{definitions}\n---@alias Identity<T> T"));
        let cycle = if name.contains('<') {
            ws.ty(name)
        } else {
            LuaType::Ref(LuaTypeDeclId::global(name))
        };
        let valid = ws.ty("Identity<Identity<string>>");
        let model = ws.analysis.semantic_model(file);
        assert!(
            matches!(
                expand_alias_head(&model, &cycle),
                None | Some(LuaType::Any | LuaType::Unknown)
            ),
            "{definitions}"
        );
        assert_eq!(expand_alias_generic(&model, &cycle), cycle, "{definitions}");
        // A failed path must release its state before an independent query.
        assert_eq!(expand_alias_generic(&model, &valid), LuaType::String);
    }
}

#[test]
fn long_valid_alias_chains_resolve_to_their_final_type() {
    for kind in ["named", "generic", "conditional"] {
        let generic = kind != "named";
        let mut ws = VirtualWorkspace::new();
        let mut source = String::new();
        for i in 0..512 {
            if kind == "conditional" {
                source.push_str(&format!(
                    "---@alias Alias{i}<T> T extends string and Alias{}<T> or never\n",
                    i + 1
                ));
            } else if generic {
                source.push_str(&format!("---@alias Alias{i}<T> Alias{}<T>\n", i + 1));
            } else {
                source.push_str(&format!("---@alias Alias{i} Alias{}\n", i + 1));
            }
        }
        source.push_str(if generic {
            "---@alias Alias512<T> T"
        } else {
            "---@alias Alias512 string"
        });
        let file = ws.def(&source);
        let ty = ws.ty(if generic { "Alias0<string>" } else { "Alias0" });
        let model = ws.analysis.semantic_model(file);
        assert_eq!(expand_alias_head(&model, &ty), Some(LuaType::String));
    }
}

#[test]
fn long_alias_chains_through_fields_resolve_the_leaf() {
    let mut ws = VirtualWorkspace::new();
    let mut source = (0..256)
        .map(|i| format!("---@alias Alias{i}<T> {{child: Alias{}<T>}}\n", i + 1))
        .collect::<String>();
    source.push_str("---@alias Alias256<T> T");
    let file = ws.def(&source);
    let ty = ws.ty("Alias0<string>");
    let model = ws.analysis.semantic_model(file);
    let expanded = expand_alias_generic(&model, &ty);
    let mut leaf = &expanded;
    for _ in 0..256 {
        let LuaType::Object(object) = leaf else {
            panic!("unresolved field: {leaf:?}")
        };
        leaf = object
            .get_fields()
            .get(&LuaMemberKey::Name("child".into()))
            .unwrap();
    }
    assert_eq!(*leaf, LuaType::String);
}

#[test]
fn operator_forwarding_resolves_long_chains_and_stops_cycles() {
    let mut ws = VirtualWorkspace::new();
    let mut source = (0..512)
        .map(|i| format!("---@alias Alias{i} Alias{}\n", i + 1))
        .collect::<String>();
    source.push_str("---@class Alias512\n---@operator len: string\n---@alias Cycle Other\n---@alias Other Cycle");
    let file = ws.def(&source);
    let model = ws.analysis.semantic_model(file);
    assert_eq!(
        model.operator_type("len", &LuaType::Ref(LuaTypeDeclId::global("Alias0"))),
        Some(LuaType::String)
    );
    assert_eq!(
        model.operator_type("len", &LuaType::Ref(LuaTypeDeclId::global("Cycle"))),
        None
    );
}

#[test]
fn alias_index_keys_resolve_finite_paths_and_stop_cycles() {
    let mut ws = VirtualWorkspace::new();
    let mut source = (0..512)
        .map(|i| format!("---@alias Key{i} Key{}\n", i + 1))
        .collect::<String>();
    source.push_str("---@alias Key512 'name'\n---@alias Keys 'name' | Keys\n---@alias Cycle Cycle\n---@class Record\n---@field name string");
    let file = ws.def(&source);
    let model = ws.analysis.semantic_model(file);
    let record = LuaType::Ref(LuaTypeDeclId::global("Record"));
    for (key, expected) in [
        ("Key0", Some(LuaType::String)),
        ("Keys", Some(LuaType::String)),
        ("Cycle", None),
    ] {
        assert_eq!(
            eval_index_access(&model, &record, &LuaType::Ref(LuaTypeDeclId::global(key))),
            expected,
            "{key}"
        );
    }
}

#[test]
fn recursive_alias_member_queries_keep_concrete_alternatives() {
    let mut ws = VirtualWorkspace::new();
    let file = ws.def(
        r#"
        ---@alias Growing<T> Growing<T[]>
        ---@alias Conditional<T> Filter<Conditional<T[]>>
        ---@alias Filter<T> T extends string and T or T[]
        ---@alias Leaf<T> {value: T}
        ---@alias Tree<T> Leaf<T> | Tree<T>[]
        ---@alias NamedTree Leaf<string> | NamedTree[]
        ---@class Owner<T>
        ---@field value T
        ---@field leaf Leaf<T>
        ---@field next Project<T[]>
        ---@alias Project<T> RawGet<Owner<T>, 'next'>
        ---@alias Field<T> RawGet<Owner<T>, 'value'>
        ---@class Computed<T>
        ---@field next RawGet<Computed<T[]>, 'next'>
        ---@alias ComputedProject<T> RawGet<Computed<T>, 'next'>
        "#,
    );
    let cycles = ["Growing", "Conditional", "Project", "ComputedProject"]
        .map(|name| ws.ty(&format!("{name}<string>")));
    let field = ws.ty("Field<string>");
    let leaf = ws.ty("RawGet<Owner<string>, 'leaf'>");
    let expected_leaf = ws.ty("{value: string}");
    let tree = ws.ty("Tree<string>");
    let named_tree = LuaType::Ref(LuaTypeDeclId::global("NamedTree"));
    let model = ws.analysis.semantic_model(file);
    let key = LuaMemberKey::Name("value".into());
    assert_eq!(expand_alias_generic(&model, &field), LuaType::String);
    assert_eq!(expand_alias_generic(&model, &leaf), expected_leaf);
    for cycle in cycles {
        assert_eq!(model.member_type(&cycle, &key), None);
        assert!(model.member_infos(&cycle).is_empty());
    }
    for tree in [tree, named_tree] {
        assert_eq!(model.member_type(&tree, &key), Some(LuaType::String));
        let members = model.member_infos(&tree);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].typ, LuaType::String);
    }
}
