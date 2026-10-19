//! Infer-layer tests: faithful projection + name/member/expression inference.

use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

use lsp_types::Uri;

use emmylua_parser::{LuaAstNode, LuaClosureExpr};

use crate::member_key::LuaMemberKey;
use crate::semantic_db::SemanticDatabase;
use crate::semantic_db::def::SemanticId;
use crate::semantic_model::SemanticModel;
use crate::{
    Emmyrc, FileId, GenericTpl, GenericTplId, InFiled, LuaInstanceType, LuaType, LuaTypeDeclId,
};

use super::unify::{TplBindings, substitute, unify_bindings};
use super::vm::{closure_param_vm, infer_expr_vm};
use crate::semantic_model::infer::infer_expr;

struct TestModel {
    db: SemanticDatabase,
    file_id: FileId,
}

impl TestModel {
    fn model(&self) -> SemanticModel<'_> {
        SemanticModel::new(&self.db, self.file_id)
    }
}

fn model_of(source: &str) -> TestModel {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SemanticDatabase::new();
    db.update_config(emmyrc);
    let uri = Uri::from_str("file:///C:/ws/infer.lua").unwrap();
    let fid = db.set_file_content(&uri, Some(source.to_string()));

    TestModel { db, file_id: fid }
}

fn decl_of(model: &SemanticModel, name: &str) -> SemanticId {
    model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == name)
        .expect("decl")
        .id
        .clone()
}

#[test]
fn test_numeric_for_len_guard_narrows_loop_body_value() {
    let env = model_of(
        "---@type false|fun(...)[]?\nlocal calls\nfor i = 1, #calls do\n    local x = calls\nend",
    );
    let model = env.model();
    let x = decl_of(&model, "x");
    let facts = model.file_facts().expect("facts");
    let decl = facts.decl_by_id(&x).expect("decl");
    let ty = model.type_of_decl_at(&x, decl.name_range.start());
    assert!(
        !ty.is_nullable(),
        "loop body `#calls` guard should remove nil: {ty:?}"
    );
    assert!(
        !matches!(ty, LuaType::Boolean | LuaType::BooleanConst(false)),
        "loop body `#calls` guard should remove false: {ty:?}"
    );
}

#[test]
fn test_repeat_until_condition_narrows_after_loop() {
    let env =
        model_of("---@type string?\nlocal x\nrepeat\n    local _ = x\nuntil x ~= nil\nlocal z = x");
    let model = env.model();
    let facts = model.file_facts().expect("facts");
    let z = decl_of(&model, "z");
    let decl = facts.decl_by_id(&z).expect("decl");
    let ty = model.type_of_decl_at(&z, decl.name_range.start());
    assert!(
        !ty.is_nullable(),
        "repeat/until 出口 `x ~= nil` 应把 x 窄化为非 nil: {ty:?}"
    );
}

#[test]
fn test_goto_edge_merges_flow_after_label() {
    let env = model_of(
        "local cond = true\nlocal x\nif cond then\n    x = 1\n    goto done\nelse\n    x = 's'\nend\n::done::\nlocal y = x",
    );
    let model = env.model();
    let facts = model.file_facts().expect("facts");
    let y = decl_of(&model, "y");
    let decl = facts.decl_by_id(&y).expect("decl");
    let ty = model.type_of_decl_at(&y, decl.name_range.start());
    let LuaType::Union(union) = ty else {
        panic!("goto 合并后应为 union，got {ty:?}");
    };
    let types = union.into_vec();
    assert!(
        types
            .iter()
            .any(|ty| matches!(ty, LuaType::IntegerConst(1)))
            && types.iter().any(|ty| matches!(ty, LuaType::StringConst(_))),
        "label 前驱应包含 goto 分支: {types:?}"
    );
}

#[test]
fn test_float_literal_projection_is_float_const() {
    let env = model_of("local x = 1.5");
    let model = env.model();
    let x = decl_of(&model, "x");
    let ty = model.type_of_decl(&x).expect("x type");
    assert_eq!(ty, LuaType::FloatConst(1.5));
}

#[test]
fn test_infer_expr_function_structure() {
    // Projection of `@type fun(a: number): string` should be a DocFunction structure
    // (for type_check).
    let env = model_of("---@type fun(a: number): string\nlocal f\nlocal r = f(1)");
    let model = env.model();
    let f = decl_of(&model, "f");
    let ty = model.type_of_decl(&f).expect("f type");
    match &ty {
        LuaType::DocFunction(fun) => {
            assert_eq!(fun.get_params().len(), 1);
            assert_eq!(fun.get_ret(), &LuaType::String);
        }
        other => panic!("f 应为 DocFunction，got {:?}", other),
    }
}

#[test]
fn test_infer_expr_member_cross_file() {
    // B defines M.x = 1; A reads M.x -> Number.
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SemanticDatabase::new();
    db.update_config(emmyrc);
    let uri_b = Uri::from_str("file:///C:/ws/b.lua").unwrap();
    let fid_b = db.set_file_content(&uri_b, Some("M = {}\nM.x = 1".to_string()));
    let uri_a = Uri::from_str("file:///C:/ws/a.lua").unwrap();
    let fid = db.set_file_content(&uri_a, Some("local y = M.x".to_string()));
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let _ = fid_b;
    let env = TestModel { db, file_id: fid };
    let model = env.model();

    let y = decl_of(&model, "y");
    assert_eq!(model.type_of_decl(&y), Some(LuaType::Number));
}

#[test]
fn test_infer_expr_by_syntax() {
    let env = model_of("local a = 'x' .. 'y'");
    let model = env.model();
    // `a`'s initializer is string concatenation -> String.
    let a = decl_of(&model, "a");
    let decl = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "a")
        .expect("a");
    let init = decl.value_expr_syntax.expect("init expr");
    assert_eq!(infer_expr(&model, init), LuaType::String);
    let _ = a;
}

#[test]
fn test_infer_name_at_offset() {
    let env = model_of("local x = 1\nlocal y = x + 1");
    let model = env.model();
    // Find the name_use offset for `x` inside `y`'s initializer.
    let x_use = model
        .name_uses()
        .expect("uses")
        .iter()
        .find(|u| u.name == "x")
        .expect("x use");
    let decl = model
        .resolve_name(x_use.syntax.get_range().start())
        .expect("resolve x");
    let ty = model.type_of_decl(&decl).expect("x type");
    assert_eq!(ty, LuaType::IntegerConst(1));
}

#[test]
fn test_unify_and_substitute() {
    use crate::{LuaArrayType, LuaType as LT};

    // TplRef(T) array param vs number[] actual arg -> T = number.
    let tpl_t = LT::TplRef(Rc::new(GenericTpl::new(
        GenericTplId::Type(0),
        "T".into(),
        None,
        None,
        false,
        None,
    )));
    let param = LT::Array(Rc::new(LuaArrayType::from_base_type(tpl_t.clone())));
    let arg = LT::Array(Rc::new(LuaArrayType::from_base_type(LT::Number)));

    let mut bindings = TplBindings::new();
    assert!(unify_bindings(&param, &arg, &mut bindings));
    assert_eq!(bindings.get(&GenericTplId::Type(0)), Some(&LT::Number));

    // Substitution: TplRef(T) -> number.
    let substituted = substitute(&tpl_t, &bindings);
    assert_eq!(substituted, LT::Number);
}

#[test]
fn test_substitute_nested_variants_keeps_instance_range() {
    use crate::{LuaGenericType, LuaType as LT};

    let tpl_t = LT::TplRef(Rc::new(GenericTpl::new(
        GenericTplId::Type(0),
        "T".into(),
        None,
        None,
        false,
        None,
    )));
    let mut bindings = TplBindings::new();
    bindings.insert(GenericTplId::Type(0), LT::Number);

    // Recursive substitution in Generic.
    let generic = LT::Generic(Rc::new(LuaGenericType::new(
        LuaTypeDeclId::global("Mock"),
        vec![tpl_t.clone()],
    )));
    let substituted = substitute(&generic, &bindings);
    let LT::Generic(sub_generic) = substituted else {
        panic!("expected Generic, got {substituted:?}");
    };
    assert_eq!(sub_generic.get_params(), &vec![LT::Number]);

    // Instance substitution must preserve the original file/range (regression: it was
    // once rebuilt as FileId(0)).
    let range = rowan::TextRange::new(rowan::TextSize::from(11), rowan::TextSize::from(22));
    let instance = LT::Instance(Rc::new(LuaInstanceType::new(
        tpl_t,
        InFiled::new(FileId::new(7), range),
    )));
    let substituted = substitute(&instance, &bindings);
    let LT::Instance(sub_instance) = substituted else {
        panic!("expected Instance, got {substituted:?}");
    };
    assert_eq!(sub_instance.get_range().file_id.id, 7);
    assert_eq!(sub_instance.get_range().value, range);
    assert_eq!(sub_instance.get_base(), &LT::Number);
}

#[test]
fn test_vm_name_binary() {
    // VM：LoadName(x) + LoadName(1) + Binary(Add) → Number。
    let env = model_of("local x = 1\nlocal y = x + 1");
    let model = env.model();
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = infer_expr_vm(&model, init);
    assert_eq!(ty, LuaType::IntegerConst(2));
}

#[test]
fn test_vm_member_access() {
    // VM：LoadName(t) + IndexMember(a) → Number。
    let env = model_of("local t = {}\nt.a = 5\nlocal y = t.a");
    let model = env.model();
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = infer_expr_vm(&model, init);
    assert_eq!(ty, LuaType::Number);
}

#[test]
fn test_vm_call() {
    // VM: LoadName(f) + Call(0) -> returns Number.
    let env = model_of("local function f() return 1 end\nlocal y = f()");
    let model = env.model();
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = infer_expr_vm(&model, init);
    assert_eq!(ty, LuaType::Number);
}

#[test]
fn test_vm_method_return_self_is_owner_type() {
    // `function B:one() return self end` -> `B:one()` returns `Ref B`.
    let env = model_of(
        "---@class B\n\
         local B = {}\n\
         function B:one() return self end\n\
         local y = B:one()",
    );
    let model = env.model();
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = infer_expr_vm(&model, init);
    assert_eq!(ty, LuaType::Ref(LuaTypeDeclId::global("B")));
}

#[test]
fn test_vm_closure_param_env() {
    // VM: closure param environment is filled via Call -- manually built with a callee
    // that has a function-typed param.
    let env = model_of("local list = {}\nlocal function each(cb) end\neach(function(v) end)");
    let model = env.model();
    // Find the closure.
    let tree = model.syntax_tree().expect("tree");
    let chunk = tree.get_chunk_node();
    let closure = chunk
        .descendants::<LuaClosureExpr>()
        .next()
        .expect("closure");
    let closure_syntax = closure.get_syntax_id();
    // M0: callee `each` has no generic doc -> param type Unknown (no panic; mechanism works).
    let ty = closure_param_vm(&model, closure_syntax, 0);
    let _ = ty;
}

#[test]
fn test_vm_generic_map_closure_params() {
    // End-to-end: fun<T>(list: T[], callback: fun(item: T, index: number): boolean): T[]
    // map(number[], function(x, y) end) -> x: number, y: number.
    let env = model_of(
        "---@type fun<T>(list: T[], callback: fun(item: T, index: number): boolean): T[]\n\
         local map = function(list, callback) end\n\
         ---@type number[]\n\
         local list = {}\n\
         map(list, function(x, y) return x end)",
    );
    let model = env.model();
    let tree = model.syntax_tree().expect("tree");
    let chunk = tree.get_chunk_node();
    // Take the closure passed as the call arg (containing `return x`), not map's own closure.
    let closure = chunk
        .descendants::<LuaClosureExpr>()
        .find(|c| {
            c.descendants::<emmylua_parser::LuaNameExpr>()
                .any(|n| n.get_name_text().as_deref() == Some("x"))
        })
        .expect("closure");
    let closure_syntax = closure.get_syntax_id();

    // Closure param back-inference: x -> number, y -> number.
    assert_eq!(closure_param_vm(&model, closure_syntax, 0), LuaType::Number);
    assert_eq!(closure_param_vm(&model, closure_syntax, 1), LuaType::Number);

    // The use of `x` inside the closure reads the environment via VM LoadName -> number.
    let x_use = closure
        .descendants::<emmylua_parser::LuaNameExpr>()
        .find(|name| name.get_name_text().as_deref() == Some("x"))
        .expect("x use in closure");
    assert_eq!(infer_expr(&model, x_use.get_syntax_id()), LuaType::Number);

    // Call return: substitute T=number into T[] -> number[].
    let call_expr = closure
        .ancestors::<emmylua_parser::LuaCallExpr>()
        .next()
        .expect("call");
    let ret = infer_expr(&model, call_expr.get_syntax_id());
    match ret {
        LuaType::Array(array) => assert_eq!(*array.get_base(), LuaType::Number),
        other => panic!("map(...) 返回应为 number[]，got {:?}", other),
    }
}

#[test]
fn test_vm_generic_tag_closure_params() {
    // `---@generic T` + `---@param` form (classic EmmyLua style):
    // map(number[], function(x, y) end) -> x: number, y: number, returns number[].
    let env = model_of(
        "---@generic T\n\
         ---@param list T[]\n\
         ---@param callback fun(item: T, index: number): boolean\n\
         ---@return T[]\n\
         local function map(list, callback) end\n\
         ---@type number[]\n\
         local list = {}\n\
         map(list, function(x, y) return x end)",
    );
    let model = env.model();
    let tree = model.syntax_tree().expect("tree");
    let chunk = tree.get_chunk_node();
    let closure = chunk
        .descendants::<LuaClosureExpr>()
        .find(|c| {
            c.descendants::<emmylua_parser::LuaNameExpr>()
                .any(|n| n.get_name_text().as_deref() == Some("x"))
        })
        .expect("closure");
    let closure_syntax = closure.get_syntax_id();

    assert_eq!(closure_param_vm(&model, closure_syntax, 0), LuaType::Number);
    assert_eq!(closure_param_vm(&model, closure_syntax, 1), LuaType::Number);
    let call_expr = closure
        .ancestors::<emmylua_parser::LuaCallExpr>()
        .next()
        .expect("call");
    let ret = infer_expr(&model, call_expr.get_syntax_id());
    match ret {
        LuaType::Array(array) => assert_eq!(*array.get_base(), LuaType::Number),
        other => panic!("map(...) 返回应为 number[]，got {:?}", other),
    }
}

#[test]
fn test_infer_call_simple() {
    // Non-generic call: function add(a, b) return a + b end -> add(1, 2) return.
    let env = model_of("local function add(a, b) return a + b end\nlocal r = add(1, 2)");
    let model = env.model();
    let r = decl_of(&model, "r");
    assert_eq!(model.type_of_decl(&r), Some(LuaType::Number));
}

// ──────────────────────────────────────────────
// find_decl / infer_expr_list_types
// ──────────────────────────────────────────────

/// Locate a token by its `occurrence`-th textual occurrence.
fn token_at(
    model: &SemanticModel,
    source: &str,
    needle: &str,
    occurrence: usize,
) -> emmylua_parser::LuaSyntaxToken {
    let byte = source
        .match_indices(needle)
        .nth(occurrence)
        .map(|(idx, _)| idx)
        .expect("needle occurrence");
    let tree = model.syntax_tree().expect("tree");
    let root = tree.get_red_root();
    match root.token_at_offset(rowan::TextSize::from(byte as u32)) {
        rowan::TokenAtOffset::Single(token) => token,
        // Name start == token start: take the right token (left is whitespace).
        rowan::TokenAtOffset::Between(_, right) => right,
        rowan::TokenAtOffset::None => panic!("no token at {}", byte),
    }
}

#[test]
fn test_vararg_semantic_info_keeps_function_ownership() {
    let source = r#"
        ---@param ... integer
        local function outer(...)
            do print(...) end
            ---@param ... string
            local function inner(...)
                print(...)
            end
            inner('hello')
            print(...)
        end
        outer(1)
    "#;
    let env = model_of(source);
    let model = env.model();
    let declarations: Vec<_> = model
        .decls()
        .unwrap()
        .iter()
        .filter(|decl| decl.name == "...")
        .collect();
    assert_eq!(declarations.len(), 2);
    for (decl, (occurrences, expected)) in declarations
        .iter()
        .zip([(vec![2, 6], LuaType::Integer), (vec![5], LuaType::String)])
    {
        for n in occurrences {
            let token = token_at(&model, source, "...", n);
            let info = model.semantic_info(token.into()).unwrap();
            assert_eq!(info.decl, Some(decl.id.clone()));
            assert_eq!(info.typ.get_result_slot_type(0), Some(expected.clone()));
        }
    }
}

#[test]
fn test_vararg_references_do_not_cross_non_variadic_functions() {
    let source = r#"
        local function outer(...)
            local function inner()
                print(...)
            end
            inner()
        end
        print(...)
    "#;
    let env = model_of(source);
    let model = env.model();
    for occurrence in [1, 2] {
        let token = token_at(&model, source, "...", occurrence);
        assert_eq!(model.find_decl(token.clone().into()), None);
        assert_eq!(model.semantic_info(token.into()).unwrap().decl, None);
    }
}

#[test]
fn test_vararg_expression_lists_expand_only_the_last_value() {
    let env = model_of(
        r#"
        ---@param ... integer
        local function values(...)
            local first, second = ..., 'last'
            local third, fourth = 1, ...
            local fifth, sixth = (...)
        end
    "#,
    );
    let model = env.model();
    let locals: Vec<_> = model
        .chunk()
        .unwrap()
        .descendants::<emmylua_parser::LuaLocalStat>()
        .collect();
    let actual: Vec<Vec<LuaType>> = locals
        .iter()
        .map(|local| {
            let exprs = local.get_value_exprs().collect::<Vec<_>>();
            model
                .infer_expr_list_types(&exprs, Some(2))
                .into_iter()
                .map(|(ty, _)| ty)
                .collect()
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            vec![
                LuaType::Integer,
                LuaType::StringConst(smol_str::SmolStr::new("last").into())
            ],
            vec![LuaType::IntegerConst(1), LuaType::Integer],
            vec![LuaType::Integer],
        ]
    );
}

#[test]
fn test_vararg_operators_take_the_first_value() {
    let env = model_of(
        r#"
        ---@param ... integer
        local function values(...)
            local negative = -...
            local choice = ... or false
        end
    "#,
    );
    let model = env.model();
    for name in ["negative", "choice"] {
        let facts = model.file_facts().unwrap();
        let decl = facts.decl_by_id(&decl_of(&model, name)).unwrap();
        assert_eq!(
            model.type_of_expr(decl.value_expr_syntax.unwrap()),
            LuaType::Integer
        );
    }
}

#[test]
fn test_vararg_packing_distinguishes_tuple_values_and_sequences() {
    let env = model_of(
        r#"
        ---@param ... [string, integer]
        local function tuples(...)
            local packed = { ... }
            local tuple_first = packed[1]
            local tuple_third = packed[3]
            local tuple_count = select('#', ...)
        end
        ---@param ... [string, integer]...
        local function sequence(...)
            local packed = { ... }
            local sequence_first = packed[1]
            local sequence_second = packed[2]
            local sequence_third = packed[3]
            local selected = { select(2, ...) }
            local selected_sequence = selected[1]
            local sequence_count = select('#', ...)
        end
        ---@return boolean, [string, integer]...
        local function returned_sequence() error('stub') end
        local returned = { returned_sequence() }
        local returned_first = returned[2]
        local returned_second = returned[3]
        ---@param ... [string, integer]
        local function forward(...) return true, ... end
        local forwarded = { forward({'a', 1}, {'b', 2}) }
        local forwarded_second = forwarded[3]
        local alias = forward
        local aliased = { alias({'a', 1}, {'b', 2}) }
        local aliased_second = aliased[3]
        ---@param ... [string, integer]...
        local function first(...) local value = ...; return value end
        local first_value = first('a', 1)
    "#,
    );
    let model = env.model();
    let tuple = LuaType::Tuple(Rc::new(crate::LuaTupleType::new(
        vec![LuaType::String, LuaType::Integer],
        crate::LuaTupleStatus::DocResolve,
    )));
    for name in [
        "tuple_first",
        "tuple_third",
        "forwarded_second",
        "aliased_second",
    ] {
        assert_eq!(
            model.type_of_decl(&decl_of(&model, name)),
            Some(tuple.clone()),
            "{name}"
        );
    }
    for (name, expected) in [
        ("sequence_first", LuaType::String),
        ("sequence_second", LuaType::Integer),
        ("sequence_third", LuaType::Nil),
        ("tuple_count", LuaType::Integer),
        ("selected_sequence", LuaType::Integer),
        ("sequence_count", LuaType::IntegerConst(2)),
        ("returned_first", LuaType::String),
        ("returned_second", LuaType::Integer),
        ("first_value", LuaType::String),
    ] {
        assert_eq!(
            model.type_of_decl(&decl_of(&model, name)),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn test_vararg_packing_expands_only_the_last_unkeyed_field() {
    let env = model_of(
        r#"
        ---@param ... integer
        local function values(...)
            local final = { 0, ... }
            local repeated = final[3]
            local nonfinal = { ..., 'last' }
            local parenthesized = { (...) }
            local keyed = { [1] = ... }
            local last = nonfinal[2]
            local missing = parenthesized[2]
            local also_missing = keyed[2]
        end
    "#,
    );
    let model = env.model();
    assert_eq!(
        model.type_of_decl(&decl_of(&model, "repeated")),
        Some(LuaType::Integer)
    );
    assert_eq!(
        model.type_of_decl(&decl_of(&model, "last")),
        Some(LuaType::StringConst(smol_str::SmolStr::new("last").into()))
    );
    for name in ["missing", "also_missing"] {
        assert_eq!(
            model.type_of_decl(&decl_of(&model, name)),
            Some(LuaType::Nil)
        );
    }
}

#[test]
fn test_vararg_return_lists_preserve_value_positions() {
    let mut ws = crate::VirtualWorkspace::new();
    ws.def(
        r#"
        ---@param ... integer
        local function forward(...) return ... end
        ---@param ... integer
        local function prefix(...) return false, ... end
        ---@param ... integer
        local function suffix(...) return ..., 'last' end
        ---@param ... integer
        local function single(...) return (...) end
        forward_first, forward_second = forward(1, 2)
        prefix_first, prefix_second, prefix_third = prefix(1, 2)
        suffix_first, suffix_second = suffix(1, 2)
        single_first = single(1, 2)
        single_count = select('#', single(1, 2))
    "#,
    );
    for name in [
        "forward_first",
        "forward_second",
        "prefix_second",
        "prefix_third",
        "suffix_first",
        "single_first",
    ] {
        assert_eq!(ws.expr_ty(name), ws.ty("integer"), "{name}");
    }
    assert_eq!(ws.expr_ty("prefix_first"), ws.ty("boolean"));
    assert_eq!(ws.expr_ty("suffix_second"), ws.ty("string"));
    assert_eq!(ws.expr_ty("single_count"), LuaType::IntegerConst(1));
}

#[test]
fn test_vararg_argument_counts_respect_single_value_contexts() {
    let env = model_of(
        r#"
        ---@param ... integer
        local function values(...)
            local all = select('#', ...)
            local one = select('#', (...))
            local two = select('#', ..., 'last')
        end
    "#,
    );
    let model = env.model();
    for (name, expected) in [
        ("all", LuaType::Integer),
        ("one", LuaType::IntegerConst(1)),
        ("two", LuaType::IntegerConst(2)),
    ] {
        assert_eq!(
            model.type_of_decl(&decl_of(&model, name)),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn test_vararg_pcall_keeps_repeated_result_types() {
    let env = model_of(
        r#"
        ---@param ... [string, integer]
        local function forward(...) return ... end
        local selected_tuple = select(3, pcall(forward, {'a', 1}, {'b', 2}))
        local protected_count = select('#', pcall(forward, {'a', 1}, {'b', 2}))
        local extended = {xpcall(forward, tostring, {'a', 1}, {'b', 2})}
        local extended_tuple = extended[3]
    "#,
    );
    let model = env.model();
    let pair = LuaType::Tuple(Rc::new(crate::LuaTupleType::new(
        vec![LuaType::String, LuaType::Integer],
        crate::LuaTupleStatus::DocResolve,
    )));
    for name in ["selected_tuple", "extended_tuple"] {
        assert_eq!(
            model.type_of_decl(&decl_of(&model, name)),
            Some(LuaType::from_vec(vec![pair.clone(), LuaType::Nil])),
            "{name}"
        );
    }
    assert_eq!(
        model.type_of_decl(&decl_of(&model, "protected_count")),
        Some(LuaType::Integer)
    );
}

#[test]
fn test_vararg_select_keeps_nested_result_types() {
    let env = model_of(
        r#"
        ---@param ... integer
        local function values(...) return true, ... end
        local count = select('#', values(1, 2, 3))
        ---@param ... integer
        local function prefixed(...)
            local prefix, tail = select(1, 'head', ...)
        end
        ---@return boolean, [string, integer]...
        local function finite() error('stub') end
        local finite_count = select('#', finite())
        local finite_third = select(3, finite())
    "#,
    );
    let model = env.model();
    for (name, expected) in [
        ("count", LuaType::Integer),
        ("prefix", LuaType::String),
        ("tail", LuaType::Integer),
        ("finite_count", LuaType::IntegerConst(3)),
        ("finite_third", LuaType::Integer),
    ] {
        assert_eq!(
            model.type_of_decl(&decl_of(&model, name)),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn test_vararg_generic_callback_keeps_result_tail() {
    let env = model_of(
        r#"
        ---@generic First, Rest
        ---@param callback fun(): First, Rest...
        ---@return First, Rest...
        local function wrap(callback) return callback() end
        ---@param ... [string, integer]
        local function tuples(...) return ... end
        local pairs = {wrap(function() return tuples({'a', 1}, {'b', 2}, {'c', 3}) end)}
        local third_pair = pairs[3]
        ---@return boolean, string, integer
        local function finite() error('stub') end
        local fixed = {wrap(finite)}
        local finite_text = fixed[2]
        local finite_number = fixed[3]
    "#,
    );
    let model = env.model();
    let pair = LuaType::Tuple(Rc::new(crate::LuaTupleType::new(
        vec![LuaType::String, LuaType::Integer],
        crate::LuaTupleStatus::DocResolve,
    )));
    for (name, expected) in [
        ("third_pair", pair),
        ("finite_text", LuaType::String),
        ("finite_number", LuaType::Integer),
    ] {
        assert_eq!(
            model.type_of_decl(&decl_of(&model, name)),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn test_vararg_index_keys_take_the_first_value() {
    let env = model_of(
        r#"
        ---@param ... 'key'
        local function values(...)
            local fields = { key = 'value' }
            local value = fields[...]
        end
    "#,
    );
    let model = env.model();
    assert_eq!(
        model.type_of_decl(&decl_of(&model, "value")),
        Some(LuaType::StringConst(smol_str::SmolStr::new("value").into()))
    );
}

#[test]
fn test_vararg_nested_returns_do_not_change_outer_result_types() {
    let mut ws = crate::VirtualWorkspace::new();
    ws.def(
        r#"
        ---@param ... integer
        local function outer(...)
            ---@param ... string
            local function inner(...) return ... end
            inner('ignored')
            return ...
        end
        first, second = outer(1, 2)
    "#,
    );
    assert_eq!(ws.expr_ty("first"), ws.ty("integer"));
    assert_eq!(ws.expr_ty("second"), ws.ty("integer"));
}

/// Name-use point -> declaration.
#[test]
fn test_find_decl_name_use() {
    let source = "local x = 1\nlocal y = x";
    let env = model_of(source);
    let model = env.model();
    let x_decl = decl_of(&model, "x");
    let token = token_at(&model, source, "x", 1);
    assert_eq!(
        model.find_decl(rowan::NodeOrToken::Token(token)),
        Some(x_decl)
    );
}

/// Definition-site name -> its own declaration.
#[test]
fn test_find_decl_definition_site() {
    let source = "local x = 1";
    let env = model_of(source);
    let model = env.model();
    let x_decl = decl_of(&model, "x");
    let token = token_at(&model, source, "x", 0);
    assert_eq!(
        model.find_decl(rowan::NodeOrToken::Token(token)),
        Some(x_decl)
    );
}

/// Index member key -> member (both use and definition sites resolve to the member itself).
#[test]
fn test_find_decl_member() {
    let source = "local t = {}\nt.z = 5\nlocal y = t.z";
    let env = model_of(source);
    let model = env.model();
    let member = model
        .members()
        .expect("members")
        .iter()
        .find(|m| m.key.name() == Some("z"))
        .expect("z member")
        .id
        .clone();
    let use_token = token_at(&model, source, "z", 1);
    assert_eq!(
        model.find_decl(rowan::NodeOrToken::Token(use_token)),
        Some(member)
    );
}

/// Doc name type -> type definition.
#[test]
fn test_find_decl_doc_name_type() {
    let source = "---@class Old\nlocal Old = {}\n---@type Old\nlocal u";
    let env = model_of(source);
    let model = env.model();
    let def = model
        .file_facts()
        .expect("facts")
        .type_def_by_name("Old")
        .expect("Old")
        .id
        .clone();
    let token = token_at(&model, source, "Old", 2);
    assert_eq!(model.find_decl(rowan::NodeOrToken::Token(token)), Some(def));
}

/// Multi-value expression list: 3 literals -> [Number, StringConst, Boolean].
#[test]
fn test_infer_expr_list_types_basic() {
    let env = model_of("local a, b, c = 1, 's', true");
    let model = env.model();
    let tree = model.syntax_tree().expect("tree");
    let chunk = tree.get_chunk_node();
    let exprs: Vec<emmylua_parser::LuaExpr> =
        chunk.descendants::<emmylua_parser::LuaExpr>().collect();
    assert_eq!(exprs.len(), 3, "3 个字面量值表达式");
    let types = model.infer_expr_list_types(&exprs, Some(3));
    assert_eq!(types.len(), 3);
    assert_eq!(types[0].0, LuaType::IntegerConst(1));
    assert!(matches!(types[1].0, LuaType::StringConst(_)));
    assert!(
        matches!(types[2].0, LuaType::BooleanConst(true)),
        "true 应为 BooleanConst: {:?}",
        types[2].0
    );
}

/// var_count truncation: 2 receiver slots -> only the first two values.
#[test]
fn test_infer_expr_list_types_truncate() {
    let env = model_of("local a, b = 1, 's', true");
    let model = env.model();
    let tree = model.syntax_tree().expect("tree");
    let chunk = tree.get_chunk_node();
    let exprs: Vec<emmylua_parser::LuaExpr> =
        chunk.descendants::<emmylua_parser::LuaExpr>().collect();
    let types = model.infer_expr_list_types(&exprs, Some(2));
    assert_eq!(types.len(), 2);
    assert_eq!(types[0].0, LuaType::IntegerConst(1));
    assert!(matches!(types[1].0, LuaType::StringConst(_)));
}

/// No var_count: all values.
#[test]
fn test_infer_expr_list_types_all() {
    let env = model_of("local a, b, c = 1, 's', true");
    let model = env.model();
    let tree = model.syntax_tree().expect("tree");
    let chunk = tree.get_chunk_node();
    let exprs: Vec<emmylua_parser::LuaExpr> =
        chunk.descendants::<emmylua_parser::LuaExpr>().collect();
    let types = model.infer_expr_list_types(&exprs, None);
    assert_eq!(types.len(), 3);
}

// ──────────────────────────────────────────────
// semantic_info
// ──────────────────────────────────────────────

/// Declaration name (definition site) -> Decl identity + type.
#[test]
fn test_semantic_info_decl_name() {
    let source = "local x = 1";
    let env = model_of(source);
    let model = env.model();
    let x_decl = decl_of(&model, "x");
    let token = token_at(&model, source, "x", 0);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::IntegerConst(1));
    assert_eq!(info.decl, Some(x_decl));
}

/// Name-use point -> Decl identity + declaration type.
#[test]
fn test_semantic_info_name_use() {
    let source = "local x = 1\nlocal y = x";
    let env = model_of(source);
    let model = env.model();
    let x_decl = decl_of(&model, "x");
    let token = token_at(&model, source, "x", 1);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::IntegerConst(1));
    assert_eq!(info.decl, Some(x_decl));
}

/// Table field key (definition site) -> Member identity + field type.
#[test]
fn test_semantic_info_table_field() {
    let source = "local t = { x = 1 }";
    let env = model_of(source);
    let model = env.model();
    let member = model
        .members()
        .expect("members")
        .iter()
        .find(|m| m.key.name() == Some("x"))
        .expect("x member")
        .id
        .clone();
    let token = token_at(&model, source, "x", 0);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::IntegerConst(1));
    assert_eq!(info.decl, Some(member));
}

/// Index member key use point -> Member identity + member type.
#[test]
fn test_semantic_info_member_use() {
    let source = "local t = {}\nt.z = 5\nlocal y = t.z";
    let env = model_of(source);
    let model = env.model();
    let member = model
        .members()
        .expect("members")
        .iter()
        .find(|m| m.key.name() == Some("z"))
        .expect("z member")
        .id
        .clone();
    let token = token_at(&model, source, "z", 1);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::Number);
    assert_eq!(info.decl, Some(member));
}

/// Index member of an instantiated generic class -> projected member type, not the declaration-site `T`.
#[test]
fn test_semantic_info_generic_member_use() {
    let source =
        "---@class MyTable<T>\n---@field aaa T\nlocal c ---@type MyTable<int>\nlocal y = c.aaa";
    let env = model_of(source);
    let model = env.model();
    let token = token_at(&model, source, "aaa", 1);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::Integer);
    assert!(
        matches!(info.decl, Some(SemanticId::Member(_))),
        "decl: {:?}",
        info.decl
    );
}

/// Two generic parameters keep their positions: `Pair<int, string>.second` is `string`, not `T`/`U`.
#[test]
fn test_semantic_info_generic_member_use_multi_param() {
    let source = "---@class Pair<T, U>\n---@field first T\n---@field second U\n\
                  local p ---@type Pair<int, string>\nlocal y = p.second";
    let env = model_of(source);
    let model = env.model();
    let token = token_at(&model, source, "second", 1);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::String);
}

/// Generic member projection also covers parameters (`---@param t MyTable<int>`) and calls.
#[test]
fn test_semantic_info_generic_member_use_param() {
    let source = "---@class MyTable<T>\n---@field aaa T\n\
                  ---@param t MyTable<int>\nlocal function f(t)\n    return t.aaa\nend";
    let env = model_of(source);
    let model = env.model();
    let token = token_at(&model, source, "aaa", 1);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::Integer);
}

/// A non-instantiated generic prefix keeps the declaration-site generic parameter (`T`).
#[test]
fn test_semantic_info_uninstantiated_generic_member_use() {
    let source = "---@class MyTable<T>\n---@field aaa T\nlocal c ---@type MyTable\nlocal y = c.aaa";
    let env = model_of(source);
    let model = env.model();
    let token = token_at(&model, source, "aaa", 1);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert!(
        matches!(info.typ, LuaType::TplRef(_) | LuaType::Unknown),
        "unbound generic member type: {:?}",
        info.typ
    );
}

/// `@field` name (inside doc comment) -> Member identity + doc type.
#[test]
fn test_semantic_info_doc_field() {
    let source = "---@class C\n---@field x number\nlocal C = {}";
    let env = model_of(source);
    let model = env.model();
    let member = model
        .members()
        .expect("members")
        .iter()
        .find(|m| m.key.name() == Some("x"))
        .expect("x field")
        .id
        .clone();
    let token = token_at(&model, source, "x", 0);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert_eq!(info.typ, LuaType::Number);
    assert_eq!(info.decl, Some(member));
}

/// Doc name type (`Old` in `---@type Old`) -> TypeDef identity + Ref type.
#[test]
fn test_semantic_info_doc_name_type() {
    let source = "---@class Old\nlocal Old = {}\n---@type Old\nlocal u";
    let env = model_of(source);
    let model = env.model();
    let def = model
        .file_facts()
        .expect("facts")
        .type_def_by_name("Old")
        .expect("Old")
        .id
        .clone();
    let token = token_at(&model, source, "Old", 2);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert!(matches!(info.typ, LuaType::Ref(_)));
    assert_eq!(info.decl, Some(def));
}

/// Literal expression -> type, no declaration identity.
#[test]
fn test_semantic_info_literal() {
    let source = "local x = 'hello'";
    let env = model_of(source);
    let model = env.model();
    let token = token_at(&model, source, "'hello'", 0);
    let info = model
        .semantic_info(rowan::NodeOrToken::Token(token))
        .expect("semantic info");
    assert!(matches!(info.typ, LuaType::StringConst(_)));
    assert_eq!(info.decl, None);
}

// ──────────────────────────────────────────────
// Table identity / setmetatable / require (item 2 inference coverage)
// ──────────────────────────────────────────────

/// Anonymous table: declaration type preserves TableConst identity; member access `t.x`
/// infers the field type.
#[test]
fn test_table_const_identity_and_member() {
    let env = model_of("local t = { x = 1 }\nlocal v = t.x");
    let model = env.model();
    let t = decl_of(&model, "t");
    let ty = model.type_of_decl(&t).expect("t type");
    assert!(
        matches!(ty, LuaType::TableConst(_)),
        "t 应为 TableConst: {:?}",
        ty
    );
    let v = decl_of(&model, "v");
    assert_eq!(model.type_of_decl(&v), Some(LuaType::IntegerConst(1)));
}

/// Nested anonymous table: `t.x.y` member chain.
#[test]
fn test_table_const_nested_member() {
    let env = model_of("local t = { x = { y = 's' } }\nlocal v = t.x.y");
    let model = env.model();
    let v = decl_of(&model, "v");
    let ty = model.type_of_decl(&v).expect("v type");
    assert!(
        matches!(ty, LuaType::String | LuaType::StringConst(_)),
        "v: {:?}",
        ty
    );
}

/// Nested table member definition lookup: `y` in `t.x.y` should resolve to the inner
/// table field declaration.
#[test]
fn test_table_const_nested_member_find_decl() {
    let env = model_of(
        "local t = { x = { y = 's' } }
local v = t.x.y",
    );
    let model = env.model();
    let chunk = model.chunk().expect("chunk");
    let y_token = chunk
        .syntax()
        .descendants_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|token| {
            token.text() == "y"
                && token
                    .parent()
                    .is_some_and(|p| p.kind() == emmylua_parser::LuaSyntaxKind::IndexExpr.into())
        })
        .expect("y token");
    let decl = model
        .find_decl(rowan::NodeOrToken::Token(y_token))
        .expect("find y decl");
    assert!(
        matches!(decl, SemanticId::Member(_)),
        "y 应解析到 Member: {:?}",
        decl
    );
}

#[test]
fn test_lambda_param_context_type_inferred() {
    let env = model_of(
        "---@param callback fun(msg: string): boolean
local function f(callback) end
f(function(msg)
    local s = msg
    return true
end)",
    );
    let model = env.model();
    let facts = model.file_facts().unwrap();
    let msg = facts
        .decls
        .iter()
        .find(|d| d.name == "msg")
        .expect("msg decl")
        .id
        .clone();
    let s = facts
        .decls
        .iter()
        .find(|d| d.name == "s")
        .expect("s decl")
        .id
        .clone();
    assert_eq!(model.type_of_decl(&msg), Some(LuaType::String));
    assert_eq!(model.type_of_decl(&s), Some(LuaType::String));
}

#[test]
fn test_array_index_type_not_unknown() {
    let env = model_of(
        "local array ---@type int[]
local v = array[1]",
    );
    let model = env.model();
    let v = decl_of(&model, "v");
    let ty = model.type_of_decl(&v).expect("array index type");
    assert!(
        !matches!(ty, LuaType::Unknown),
        "array[1] 不应是 Unknown: {:?}",
        ty
    );
}

/// TableConst member query (completion scenario).
#[test]
fn test_table_const_member_infos() {
    let env = model_of("local t = { x = 1, name = 'n' }");
    let model = env.model();
    let t = decl_of(&model, "t");
    let ty = model.type_of_decl(&t).expect("t type");
    let infos = model.member_infos(&ty);
    let names: Vec<&str> = infos
        .iter()
        .filter_map(|i| match &i.key {
            LuaMemberKey::Name(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    assert!(names.contains(&"x"), "members: {:?}", names);
    assert!(names.contains(&"name"), "members: {:?}", names);
}

/// setmetatable: returns the first table arg and passes its identity through (`t.x`
/// can be inferred).
#[test]
fn test_setmetatable_passthrough() {
    let env = model_of("local t = setmetatable({ x = 1 }, {})\nlocal v = t.x");
    let model = env.model();
    let t = decl_of(&model, "t");
    let ty = model.type_of_decl(&t).expect("t type");
    assert!(
        matches!(ty, LuaType::TableConst(_)),
        "setmetatable 应透传表身份: {:?}",
        ty
    );
    let v = decl_of(&model, "v");
    assert_eq!(model.type_of_decl(&v), Some(LuaType::IntegerConst(1)));
}

/// require variable passing: the variable is a string constant -> module name resolution.
#[test]
fn test_require_via_variable() {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SemanticDatabase::new();
    db.update_config(emmyrc);
    let uri_b = Uri::from_str("file:///C:/ws/b.lua").unwrap();
    let fid_b = db.set_file_content(&uri_b, Some("return { value = 42 }".to_string()));
    let uri_a = Uri::from_str("file:///C:/ws/a.lua").unwrap();
    let fid = db.set_file_content(
        &uri_a,
        Some("local name = 'b'\nlocal m = require(name)\nlocal v = m.value".to_string()),
    );
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let _ = fid_b;
    let env = TestModel { db, file_id: fid };
    let model = env.model();

    let v = decl_of(&model, "v");
    let ty = model.type_of_decl(&v).expect("v type");
    assert_eq!(
        ty,
        LuaType::IntegerConst(42),
        "require(name) → 模块表 → value: {:?}",
        ty
    );
}

/// require literal (VM layer): module export type.
#[test]
fn test_require_literal_vm() {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SemanticDatabase::new();
    db.update_config(emmyrc);
    let uri_b = Uri::from_str("file:///C:/ws/b.lua").unwrap();
    let fid_b = db.set_file_content(&uri_b, Some("return { value = 42 }".to_string()));
    let uri_a = Uri::from_str("file:///C:/ws/a.lua").unwrap();
    let fid = db.set_file_content(
        &uri_a,
        Some("local m = require('b')\nlocal v = m.value".to_string()),
    );
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let _ = fid_b;
    let env = TestModel { db, file_id: fid };
    let model = env.model();

    let v = decl_of(&model, "v");
    let ty = model.type_of_decl(&v).expect("v type");
    assert_eq!(
        ty,
        LuaType::IntegerConst(42),
        "require('b') → 模块表 → value: {:?}",
        ty
    );
}

#[test]
fn test_cross_file_member_cycles_keep_independent_fields() {
    for (value, expected) in [
        ("42", LuaType::IntegerConst(42)),
        ("a.value", LuaType::Unknown),
    ] {
        let mut ws = crate::VirtualWorkspace::new();
        ws.def_files(vec![
            (
                "a.lua",
                "local b = require('b')\nreturn { value = b.value, known = 'a' }",
            ),
            (
                "b.lua",
                &format!("local a = require('a')\nreturn {{ value = {value}, known = 42 }}"),
            ),
        ]);
        let file_id = ws.def("local value = require('a').value\nlocal known = require('b').known");
        let model = ws.analysis.semantic_model(file_id);
        assert_eq!(
            model.type_of_decl(&decl_of(&model, "value")),
            Some(expected)
        );
        assert_eq!(
            model.type_of_decl(&decl_of(&model, "known")),
            Some(LuaType::IntegerConst(42))
        );
    }
}

// ──────────────────────────────────────────────
// is_reference_to / is_visible
// ──────────────────────────────────────────────

/// Name-use points are references; definition sites are too (both sides match in rename).
#[test]
fn test_is_reference_to_name() {
    let source = "local x = 1\nlocal y = x\nlocal z = y";
    let env = model_of(source);
    let model = env.model();
    let x_decl = decl_of(&model, "x");
    let use_token = token_at(&model, source, "x", 1);
    assert!(model.is_reference_to(rowan::NodeOrToken::Token(use_token), &x_decl));
    let def_token = token_at(&model, source, "x", 0);
    assert!(model.is_reference_to(rowan::NodeOrToken::Token(def_token), &x_decl));
    let y_token = token_at(&model, source, "y", 1);
    assert!(!model.is_reference_to(rowan::NodeOrToken::Token(y_token), &x_decl));
}

/// Member-use points are member references.
#[test]
fn test_is_reference_to_member() {
    let source = "local t = {}\nt.z = 5\nlocal y = t.z";
    let env = model_of(source);
    let model = env.model();
    let member = model
        .members()
        .expect("members")
        .iter()
        .find(|m| m.key.name() == Some("z"))
        .expect("z member")
        .id
        .clone();
    let use_token = token_at(&model, source, "z", 1);
    assert!(model.is_reference_to(rowan::NodeOrToken::Token(use_token), &member));
}

/// `@private` field: visible in the same file, invisible from other files.
#[test]
fn test_is_visible_private_field() {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = SemanticDatabase::new();
    db.update_config(emmyrc);
    // Defining file: ---@field private secret number (visibility prefix syntax).
    let uri_b = Uri::from_str("file:///C:/ws/b.lua").unwrap();
    let fid_b = db.set_file_content(
        &uri_b,
        Some("---@class C\n---@field private secret number\nlocal C = {}".to_string()),
    );
    // Using file.
    let uri_a = Uri::from_str("file:///C:/ws/a.lua").unwrap();
    let fid = db.set_file_content(&uri_a, Some("local c = {}\nlocal v = c.secret".to_string()));
    db.update_main_root(std::path::PathBuf::from("C:/ws"));

    let env = TestModel { db, file_id: fid_b };
    let model_b = env.model();
    let model = SemanticModel::new(&env.db, fid);

    let member = model_b
        .members()
        .expect("members")
        .iter()
        .find(|m| m.key.name() == Some("secret"))
        .expect("secret field")
        .id
        .clone();
    assert_eq!(
        model_b
            .file_facts()
            .expect("facts b")
            .member_by_id(&member)
            .expect("member")
            .visibility,
        emmylua_parser::VisibilityKind::Private,
        "visibility 注解应提取为 Private"
    );

    // Access from the using file -> not visible.
    let use_token = token_at(&model, "local c = {}\nlocal v = c.secret", "secret", 0);
    assert!(!model.is_visible(rowan::NodeOrToken::Token(use_token), &member));
    // Access within the defining file -> visible (M0 same-file rule).
    let def_token = token_at(
        &model_b,
        "---@class C\n---@field private secret number\nlocal C = {}",
        "secret",
        0,
    );
    assert!(model_b.is_visible(rowan::NodeOrToken::Token(def_token), &member));
}

/// Public fields are always visible.
#[test]
fn test_is_visible_public_field() {
    let env = model_of("---@class C\n---@field x number\nlocal C = {}");
    let model = env.model();
    let member = model
        .members()
        .expect("members")
        .iter()
        .find(|m| m.key.name() == Some("x"))
        .expect("x field")
        .id
        .clone();
    let token = token_at(
        &model,
        "---@class C\n---@field x number\nlocal C = {}",
        "x",
        0,
    );
    assert!(model.is_visible(rowan::NodeOrToken::Token(token), &member));
}

// ──────────────────────────────────────────────
// type_of_decl_at (assignment-flow aware)
// ──────────────────────────────────────────────

/// Last use point of a name (skipping assignment left-hand sides etc.).
fn last_name_use(model: &SemanticModel, name: &str) -> emmylua_parser::LuaSyntaxId {
    model
        .name_uses()
        .expect("uses")
        .iter()
        .rfind(|u| u.name == name)
        .expect("use")
        .syntax
}

/// After reassignment: use-point type = type from the most recent assignment.
#[test]
fn test_type_of_decl_at_reassignment() {
    let source = "local x = 1\nx = 's'\nlocal y = x";
    let env = model_of(source);
    let model = env.model();
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    assert!(
        matches!(ty, LuaType::String | LuaType::StringConst(_)),
        "x 在再赋值后应为 string: {:?}",
        ty
    );
}

/// Immediately after declaration: initial type.
#[test]
fn test_type_of_decl_at_initial() {
    let source = "local x = 1\nlocal y = x";
    let env = model_of(source);
    let model = env.model();
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    assert_eq!(ty, LuaType::IntegerConst(1));
}

/// After branch assignment: type is a union.
#[test]
fn test_type_of_decl_at_branch_union() {
    let source = "local x = 1\nif cond then\n    x = 's'\nend\nlocal y = x";
    let env = model_of(source);
    let model = env.model();
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    match &ty {
        LuaType::Union(union) => {
            let types = union.into_vec();
            assert!(
                types.contains(&LuaType::IntegerConst(1)),
                "union 含 number: {:?}",
                types
            );
            assert!(
                types
                    .iter()
                    .any(|t| matches!(t, LuaType::String | LuaType::StringConst(_))),
                "union 含 string: {:?}",
                types
            );
        }
        other => panic!("应为并集: {:?}", other),
    }
}

/// `type(x) == 'string'` guard: narrows to string inside the block.
#[test]
fn test_type_of_decl_at_type_guard() {
    let source =
        "---@type string|number\nlocal x\nif type(x) == 'string' then\n    local y = x\nend";
    let env = model_of(source);
    let model = env.model();
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    assert_eq!(ty, LuaType::String, "type 守卫内应为 string: {:?}", ty);
}

/// Outside the guard branch: union unchanged.
#[test]
fn test_type_of_decl_at_type_guard_outside() {
    let source = "---@type string|number\nlocal x\nif type(x) == 'string' then\n    local y = x\nend\nlocal z = x";
    let env = model_of(source);
    let model = env.model();
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    match &ty {
        LuaType::Union(union) => {
            let types = union.into_vec();
            assert!(
                types.contains(&LuaType::String),
                "union 含 string: {:?}",
                types
            );
            assert!(
                types.contains(&LuaType::Number),
                "union 含 number: {:?}",
                types
            );
        }
        other => panic!("应为并集: {:?}", other),
    }
}

/// `TypeGuard<T>` return-type guard: the true branch narrows to T.
#[test]
fn test_type_guard_generic_narrows() {
    let source = "---@alias TypeGuard<T> boolean
---@param x any
---@return TypeGuard<string>
local function isStr(x) return true end

---@type any
local v
if isStr(v) then
    local s = v
end";
    let env = model_of(source);
    let model = env.model();
    let v = decl_of(&model, "v");
    let v_use = last_name_use(&model, "v");
    let ty = model.type_of_decl_at(&v, v_use.get_range().start());
    assert_eq!(
        ty,
        LuaType::String,
        "TypeGuard 真分支应窄化为 string: {:?}",
        ty
    );
}

/// `---@return_cast x string else number`: the true branch narrows to string.
#[test]
fn test_return_cast_true_branch_narrows() {
    let source = "---@param x any
---@return boolean
---@return_cast x string else number
local function isStr(x) return true end

---@type any
local v
if isStr(v) then
    local s = v
end";
    let env = model_of(source);
    let model = env.model();
    let v = decl_of(&model, "v");
    let v_use = last_name_use(&model, "v");
    let ty = model.type_of_decl_at(&v, v_use.get_range().start());
    assert_eq!(
        ty,
        LuaType::String,
        "return_cast 真分支应窄化为 string: {:?}",
        ty
    );
}

/// `---@return_cast x string else number`: the false branch narrows to number.
#[test]
fn test_return_cast_false_branch_narrows() {
    let source = "---@param x any
---@return boolean
---@return_cast x string else number
local function isStr(x) return true end

---@type any
local v
if isStr(v) then
else
    local n = v
end";
    let env = model_of(source);
    let model = env.model();
    let v = decl_of(&model, "v");
    let v_use = last_name_use(&model, "v");
    let ty = model.type_of_decl_at(&v, v_use.get_range().start());
    assert_eq!(
        ty,
        LuaType::Number,
        "return_cast 假分支应窄化为 number: {:?}",
        ty
    );
}

/// `---@return_cast self Player else Monster`: colon method's true branch narrows the receiver.
#[test]
fn test_return_cast_self_true_branch_narrows() {
    let source = "---@class Player
---@class Monster
---@class Obj
---@return boolean
---@return_cast self Player else Monster
function Obj:isP() return true end

---@type Obj
local o
if o:isP() then
    local p = o
end";
    let env = model_of(source);
    let model = env.model();
    let o = decl_of(&model, "o");
    let o_use = last_name_use(&model, "o");
    let ty = model.type_of_decl_at(&o, o_use.get_range().start());
    assert!(
        matches!(ty, LuaType::Ref(_)),
        "return_cast self 真分支应窄化为 Player: {:?}",
        ty
    );
}

/// `x == nil` guard: inside the block `x` is nil.
#[test]
fn test_type_of_decl_at_nil_guard() {
    let source = "---@type string|nil\nlocal x\nif x == nil then\n    local y = x\nend";
    let env = model_of(source);
    let model = env.model();
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    assert_eq!(ty, LuaType::Nil, "nil 守卫内应为 nil: {:?}", ty);
}

// ──────────────────────────────────────────────
// Operator overloading (---@operator)
// ──────────────────────────────────────────────

/// `---@operator add(Vector): Vector`：a + b → Vector。
#[test]
fn test_operator_add() {
    let env = model_of(
        "---@class Vector\n---@operator add(Vector): Vector\nlocal Vector = {}\n---@type Vector\nlocal a\n---@type Vector\nlocal b\nlocal c = a + b",
    );
    let model = env.model();
    let c = decl_of(&model, "c");
    let ty = model.type_of_decl(&c).expect("c type");
    assert!(
        matches!(ty, LuaType::Ref(_)),
        "a + b 应为 Vector ref: {:?}",
        ty
    );
}

/// Unary `-a` (unm overload) -> Vector.
#[test]
fn test_operator_unm() {
    let env = model_of(
        "---@class Vector\n---@operator unm: Vector\nlocal Vector = {}\n---@type Vector\nlocal a\nlocal b = -a",
    );
    let model = env.model();
    let b = decl_of(&model, "b");
    let ty = model.type_of_decl(&b).expect("b type");
    assert!(
        matches!(ty, LuaType::Ref(_)),
        "-a 应为 Vector ref: {:?}",
        ty
    );
}

/// No-overload type: `number + number` is still number.
#[test]
fn test_operator_fallback_number() {
    let env = model_of("local a = 1\nlocal b = 2\nlocal c = a + b");
    let model = env.model();
    let c = decl_of(&model, "c");
    assert_eq!(model.type_of_decl(&c), Some(LuaType::IntegerConst(3)));
}

/// `#v` (len overload) -> number.
#[test]
fn test_operator_len() {
    let env = model_of(
        "---@class Vector\n---@operator len: number\nlocal Vector = {}\n---@type Vector\nlocal a\nlocal n = #a",
    );
    let model = env.model();
    let n = decl_of(&model, "n");
    assert_eq!(model.type_of_decl(&n), Some(LuaType::Number));
}
