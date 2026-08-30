//! Infer-layer tests: faithful projection + name/member/expression inference.

use std::str::FromStr;
use std::sync::Arc;

use lsp_types::Uri;

use emmylua_parser::{LuaAstNode, LuaClosureExpr};

use crate::member_key::LuaMemberKey;
use crate::salsa_builder::def::SemanticId;
use crate::semantic_model::SemanticModel;
use crate::{Emmyrc, LuaType};

use super::super::infer::infer_expr;

fn model_of(source: &str) -> (&'static SemanticModel<'static>, Arc<Emmyrc>) {
    let emmyrc = Arc::new(Emmyrc::default());
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri = Uri::from_str("file:///C:/ws/infer.lua").unwrap();
    let fid = db.set_file_content(&uri, Some(source.to_string()));
    // Leak for tests.
    let db: &'static crate::salsa_builder::SalsaDatabase = Box::leak(Box::new(db));
    let model: &'static SemanticModel<'static> =
        Box::leak(Box::new(SemanticModel::new(db, fid).unwrap()));
    (model, emmyrc)
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
    let (model, _) = model_of(
        "---@type false|fun(...)[]?\nlocal calls\nfor i = 1, #calls do\n    local x = calls\nend",
    );
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
    let (model, _) =
        model_of("---@type string?\nlocal x\nrepeat\n    local _ = x\nuntil x ~= nil\nlocal z = x");
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
    let (model, _) = model_of(
        "local cond = true\nlocal x\nif cond then\n    x = 1\n    goto done\nelse\n    x = 's'\nend\n::done::\nlocal y = x",
    );
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
    let (model, _) = model_of("local x = 1.5");
    let x = decl_of(&model, "x");
    let ty = model.type_of_decl(&x).expect("x type");
    assert_eq!(ty, LuaType::FloatConst(1.5));
}

#[test]
fn test_infer_expr_function_structure() {
    // Projection of `@type fun(a: number): string` should be a DocFunction structure
    // (for type_check).
    let (model, _) = model_of("---@type fun(a: number): string\nlocal f\nlocal r = f(1)");
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
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri_b = Uri::from_str("file:///C:/ws/b.lua").unwrap();
    let fid_b = db.set_file_content(&uri_b, Some("M = {}\nM.x = 1".to_string()));
    let uri_a = Uri::from_str("file:///C:/ws/a.lua").unwrap();
    let fid = db.set_file_content(&uri_a, Some("local y = M.x".to_string()));
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let _ = fid_b;
    let db: &'static crate::salsa_builder::SalsaDatabase = Box::leak(Box::new(db));
    let model: &'static SemanticModel<'static> =
        Box::leak(Box::new(SemanticModel::new(db, fid).unwrap()));

    let y = decl_of(&model, "y");
    assert_eq!(model.type_of_decl(&y), Some(LuaType::Number));
}

#[test]
fn test_infer_expr_by_syntax() {
    let (model, _) = model_of("local a = 'x' .. 'y'");
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
    let (model, _) = model_of("local x = 1\nlocal y = x + 1");
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
    let tpl_t = LT::TplRef(Arc::new(crate::GenericTpl::new(
        crate::GenericTplId::Type(0),
        "T".into(),
        None,
        None,
        false,
        None,
    )));
    let param = LT::Array(Arc::new(LuaArrayType::from_base_type(tpl_t.clone())));
    let arg = LT::Array(Arc::new(LuaArrayType::from_base_type(LT::Number)));

    let mut bindings = super::unify::TplBindings::new();
    assert!(super::unify::unify_bindings(&param, &arg, &mut bindings));
    assert_eq!(
        bindings.get(&crate::GenericTplId::Type(0)),
        Some(&LT::Number)
    );

    // Substitution: TplRef(T) -> number.
    let substituted = super::unify::substitute(&tpl_t, &bindings);
    assert_eq!(substituted, LT::Number);
}

#[test]
fn test_substitute_nested_variants_keeps_instance_range() {
    use crate::{LuaGenericType, LuaType as LT};

    let tpl_t = LT::TplRef(Arc::new(crate::GenericTpl::new(
        crate::GenericTplId::Type(0),
        "T".into(),
        None,
        None,
        false,
        None,
    )));
    let mut bindings = super::unify::TplBindings::new();
    bindings.insert(crate::GenericTplId::Type(0), LT::Number);

    // Recursive substitution in Generic.
    let generic = LT::Generic(Arc::new(LuaGenericType::new(
        crate::LuaTypeDeclId::global("Mock"),
        vec![tpl_t.clone()],
    )));
    let substituted = super::unify::substitute(&generic, &bindings);
    let LT::Generic(sub_generic) = substituted else {
        panic!("expected Generic, got {substituted:?}");
    };
    assert_eq!(sub_generic.get_params(), &vec![LT::Number]);

    // Instance substitution must preserve the original file/range (regression: it was
    // once rebuilt as FileId(0)).
    let range = rowan::TextRange::new(rowan::TextSize::from(11), rowan::TextSize::from(22));
    let instance = LT::Instance(Arc::new(crate::LuaInstanceType::new(
        tpl_t,
        crate::InFiled::new(crate::FileId::new(7), range),
    )));
    let substituted = super::unify::substitute(&instance, &bindings);
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
    let (model, _) = model_of("local x = 1\nlocal y = x + 1");
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = super::vm::infer_expr_vm(&model, init);
    assert_eq!(ty, LuaType::IntegerConst(2));
}

#[test]
fn test_vm_member_access() {
    // VM：LoadName(t) + IndexMember(a) → Number。
    let (model, _) = model_of("local t = {}\nt.a = 5\nlocal y = t.a");
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = super::vm::infer_expr_vm(&model, init);
    assert_eq!(ty, LuaType::Number);
}

#[test]
fn test_vm_call() {
    // VM: LoadName(f) + Call(0) -> returns Number.
    let (model, _) = model_of("local function f() return 1 end\nlocal y = f()");
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = super::vm::infer_expr_vm(&model, init);
    assert_eq!(ty, LuaType::Number);
}

#[test]
fn test_vm_method_return_self_is_owner_type() {
    // `function B:one() return self end` -> `B:one()` returns `Ref B`.
    let (model, _) = model_of(
        "---@class B\n\
         local B = {}\n\
         function B:one() return self end\n\
         local y = B:one()",
    );
    let y = model
        .decls()
        .expect("decls")
        .iter()
        .find(|d| d.name == "y")
        .expect("y");
    let init = y.value_expr_syntax.expect("init");
    let ty = super::vm::infer_expr_vm(model, init);
    assert_eq!(ty, LuaType::Ref(crate::LuaTypeDeclId::global("B")));
}

#[test]
fn test_vm_closure_param_env() {
    // VM: closure param environment is filled via Call -- manually built with a callee
    // that has a function-typed param.
    let (model, _) =
        model_of("local list = {}\nlocal function each(cb) end\neach(function(v) end)");
    // Find the closure.
    let tree = model.syntax_tree().expect("tree");
    let chunk = tree.get_chunk_node();
    let closure = chunk
        .descendants::<LuaClosureExpr>()
        .next()
        .expect("closure");
    let closure_syntax = closure.get_syntax_id();
    // M0: callee `each` has no generic doc -> param type Unknown (no panic; mechanism works).
    let ty = super::vm::closure_param_vm(&model, closure_syntax, 0);
    let _ = ty;
}

#[test]
fn test_vm_generic_map_closure_params() {
    // End-to-end: fun<T>(list: T[], callback: fun(item: T, index: number): boolean): T[]
    // map(number[], function(x, y) end) -> x: number, y: number.
    let (model, _) = model_of(
        "---@type fun<T>(list: T[], callback: fun(item: T, index: number): boolean): T[]\n\
         local map = function(list, callback) end\n\
         ---@type number[]\n\
         local list = {}\n\
         map(list, function(x, y) return x end)",
    );
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
    assert_eq!(
        super::vm::closure_param_vm(&model, closure_syntax, 0),
        LuaType::Number
    );
    assert_eq!(
        super::vm::closure_param_vm(&model, closure_syntax, 1),
        LuaType::Number
    );

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
    let (model, _) = model_of(
        "---@generic T\n\
         ---@param list T[]\n\
         ---@param callback fun(item: T, index: number): boolean\n\
         ---@return T[]\n\
         local function map(list, callback) end\n\
         ---@type number[]\n\
         local list = {}\n\
         map(list, function(x, y) return x end)",
    );
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

    assert_eq!(
        super::vm::closure_param_vm(&model, closure_syntax, 0),
        LuaType::Number
    );
    assert_eq!(
        super::vm::closure_param_vm(&model, closure_syntax, 1),
        LuaType::Number
    );
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
    let (model, _) = model_of("local function add(a, b) return a + b end\nlocal r = add(1, 2)");
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

/// Name-use point -> declaration.
#[test]
fn test_find_decl_name_use() {
    let source = "local x = 1\nlocal y = x";
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of("local a, b, c = 1, 's', true");
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
    let (model, _) = model_of("local a, b = 1, 's', true");
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
    let (model, _) = model_of("local a, b, c = 1, 's', true");
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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

/// `@field` name (inside doc comment) -> Member identity + doc type.
#[test]
fn test_semantic_info_doc_field() {
    let source = "---@class C\n---@field x number\nlocal C = {}";
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of("local t = { x = 1 }\nlocal v = t.x");
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
    let (model, _) = model_of("local t = { x = { y = 's' } }\nlocal v = t.x.y");
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
    let (model, _) = model_of(
        "local t = { x = { y = 's' } }
local v = t.x.y",
    );
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
    let (model, _) = model_of(
        "---@param callback fun(msg: string): boolean
local function f(callback) end
f(function(msg)
    local s = msg
    return true
end)",
    );
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
    let (model, _) = model_of(
        "local array ---@type int[]
local v = array[1]",
    );
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
    let (model, _) = model_of("local t = { x = 1, name = 'n' }");
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
    let (model, _) = model_of("local t = setmetatable({ x = 1 }, {})\nlocal v = t.x");
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
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri_b = Uri::from_str("file:///C:/ws/b.lua").unwrap();
    let fid_b = db.set_file_content(&uri_b, Some("return { value = 42 }".to_string()));
    let uri_a = Uri::from_str("file:///C:/ws/a.lua").unwrap();
    let fid = db.set_file_content(
        &uri_a,
        Some("local name = 'b'\nlocal m = require(name)\nlocal v = m.value".to_string()),
    );
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let _ = fid_b;
    let db: &'static crate::salsa_builder::SalsaDatabase = Box::leak(Box::new(db));
    let model: &'static SemanticModel<'static> =
        Box::leak(Box::new(SemanticModel::new(db, fid).unwrap()));

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
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri_b = Uri::from_str("file:///C:/ws/b.lua").unwrap();
    let fid_b = db.set_file_content(&uri_b, Some("return { value = 42 }".to_string()));
    let uri_a = Uri::from_str("file:///C:/ws/a.lua").unwrap();
    let fid = db.set_file_content(
        &uri_a,
        Some("local m = require('b')\nlocal v = m.value".to_string()),
    );
    db.update_main_root(std::path::PathBuf::from("C:/ws"));
    let _ = fid_b;
    let db: &'static crate::salsa_builder::SalsaDatabase = Box::leak(Box::new(db));
    let model: &'static SemanticModel<'static> =
        Box::leak(Box::new(SemanticModel::new(db, fid).unwrap()));

    let v = decl_of(&model, "v");
    let ty = model.type_of_decl(&v).expect("v type");
    assert_eq!(
        ty,
        LuaType::IntegerConst(42),
        "require('b') → 模块表 → value: {:?}",
        ty
    );
}

// ──────────────────────────────────────────────
// is_reference_to / is_visible
// ──────────────────────────────────────────────

/// Name-use points are references; definition sites are too (both sides match in rename).
#[test]
fn test_is_reference_to_name() {
    let source = "local x = 1\nlocal y = x\nlocal z = y";
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
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
    let db: &'static crate::salsa_builder::SalsaDatabase = Box::leak(Box::new(db));
    let model: &'static SemanticModel<'static> =
        Box::leak(Box::new(SemanticModel::new(db, fid).unwrap()));
    let db_b = Box::leak(Box::new(db.clone()));
    let model_b: &'static SemanticModel<'static> =
        Box::leak(Box::new(SemanticModel::new(db_b, fid_b).unwrap()));

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
    let (model, _) = model_of("---@class C\n---@field x number\nlocal C = {}");
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    assert_eq!(ty, LuaType::IntegerConst(1));
}

/// After branch assignment: type is a union.
#[test]
fn test_type_of_decl_at_branch_union() {
    let source = "local x = 1\nif cond then\n    x = 's'\nend\nlocal y = x";
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
    let x = decl_of(&model, "x");
    let x_use = last_name_use(&model, "x");
    let ty = model.type_of_decl_at(&x, x_use.get_range().start());
    assert_eq!(ty, LuaType::String, "type 守卫内应为 string: {:?}", ty);
}

/// Outside the guard branch: union unchanged.
#[test]
fn test_type_of_decl_at_type_guard_outside() {
    let source = "---@type string|number\nlocal x\nif type(x) == 'string' then\n    local y = x\nend\nlocal z = x";
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(source);
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
    let (model, _) = model_of(
        "---@class Vector\n---@operator add(Vector): Vector\nlocal Vector = {}\n---@type Vector\nlocal a\n---@type Vector\nlocal b\nlocal c = a + b",
    );
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
    let (model, _) = model_of(
        "---@class Vector\n---@operator unm: Vector\nlocal Vector = {}\n---@type Vector\nlocal a\nlocal b = -a",
    );
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
    let (model, _) = model_of("local a = 1\nlocal b = 2\nlocal c = a + b");
    let c = decl_of(&model, "c");
    assert_eq!(model.type_of_decl(&c), Some(LuaType::IntegerConst(3)));
}

/// `#v` (len overload) -> number.
#[test]
fn test_operator_len() {
    let (model, _) = model_of(
        "---@class Vector\n---@operator len: number\nlocal Vector = {}\n---@type Vector\nlocal a\nlocal n = #a",
    );
    let n = decl_of(&model, "n");
    assert_eq!(model.type_of_decl(&n), Some(LuaType::Number));
}
