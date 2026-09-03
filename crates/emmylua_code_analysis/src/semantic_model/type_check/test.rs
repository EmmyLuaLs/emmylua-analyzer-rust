//! Type-check tests (LuaType + salsa context).

use smol_str::SmolStr;

use crate::{Arc, AsyncState, LuaFunctionType, LuaType, LuaTypeDeclId, LuaUnionType};

use super::check_type_detail;
use super::context::TypeCheckContext;
use super::{check_general_type_compact, guard::TypeCheckGuard};

/// Model-less context (structural tests don't need resolution).
fn ctx() -> TypeCheckContext<'static> {
    // Borrow a dummy model: structural checks don't trigger resolution, so use a leaked fake model.
    let model: &'static crate::semantic_model::SemanticModel<'static> = Box::leak(Box::new(
        crate::semantic_model::SemanticModel::new(
            Box::leak(Box::new(crate::salsa_builder::SalsaDatabase::new())),
            crate::FileId::new(0),
        )
        .unwrap(),
    ));
    TypeCheckContext::new(model, false)
}

fn ok(source: &LuaType, target: &LuaType) -> bool {
    let mut context = ctx();
    check_general_type_compact(&mut context, source, target, TypeCheckGuard::new()).is_ok()
}

fn doc_fun(params: Vec<Option<LuaType>>, ret: LuaType) -> LuaType {
    LuaType::DocFunction(Arc::new(LuaFunctionType::new(
        AsyncState::None,
        false,
        false,
        params.into_iter().map(|t| (String::new(), t)).collect(),
        ret,
        None,
    )))
}

fn union(types: Vec<LuaType>) -> LuaType {
    LuaType::Union(Arc::new(LuaUnionType::from_vec(types)))
}

#[test]
fn test_primitives_and_consts() {
    use LuaType as LT;
    assert!(
        ok(&LT::Number, &LT::Integer),
        "旧语义：Number 可赋给 Integer"
    );
    assert!(!ok(&LT::Integer, &LT::Number));
    assert!(!ok(&LT::String, &LT::Number));
    assert!(ok(&LT::IntegerConst(1), &LT::Integer));
    assert!(ok(&LT::Number, &LT::IntegerConst(1)));
    assert!(ok(&LT::StringConst(SmolStr::new("x").into()), &LT::String));
    assert!(ok(&LT::Nil, &LT::Nil));
    assert!(!ok(&LT::Nil, &LT::Number));
    assert!(ok(&LT::Table, &LT::Table));
    assert!(ok(
        &LT::Table,
        &LT::Array(Arc::new(crate::LuaArrayType::from_base_type(LT::Number)))
    ));
    assert!(ok(
        &LT::Function,
        &LT::DocFunction(Arc::new(LuaFunctionType::new(
            AsyncState::None,
            false,
            false,
            vec![],
            LT::Nil,
            None
        )))
    ));
    assert!(ok(
        &LT::DocStringConst(SmolStr::new("a").into()),
        &LT::DocStringConst(SmolStr::new("a").into())
    ));
    assert!(!ok(
        &LT::DocStringConst(SmolStr::new("a").into()),
        &LT::DocStringConst(SmolStr::new("b").into())
    ));
}

#[test]
fn test_never_is_bottom() {
    use LuaType as LT;
    assert!(ok(&LT::Never, &LT::String));
    assert!(ok(&LT::Never, &LT::Number));
    assert!(ok(&LT::Never, &LT::Nil));
    assert!(ok(&LT::Never, &LT::Table));
    assert!(!ok(&LT::String, &LT::Never));
}

#[test]
fn test_union_intersection() {
    use LuaType as LT;
    // Target union: source matches any one component.
    let t_union = union(vec![LT::String, LT::Number]);
    assert!(!ok(&LT::String, &t_union), "target 并集要求全部接受");
    assert!(!ok(&LT::Boolean, &t_union));
    // Source union: any member matching is enough.
    let s_union = union(vec![LT::String, LT::Number]);
    assert!(ok(&s_union, &LT::Number));
    assert!(!ok(&s_union, &LT::Boolean));
    // Nullable: `integer?` is assignable to `integer`.
    let nullable = LT::Union(Arc::new(LuaUnionType::Nullable(LT::Integer)));
    assert!(ok(&nullable, &LT::Integer));
}

#[test]
fn test_function_structure() {
    use LuaType as LT;
    // Return covariance.
    assert!(
        ok(&doc_fun(vec![], LT::Number), &doc_fun(vec![], LT::Integer)),
        "返回协变（旧数值宽松语义）"
    );
    assert!(
        !ok(&doc_fun(vec![], LT::String), &doc_fun(vec![], LT::Integer)),
        "返回不匹配"
    );
    // Parameters: the compact parameter type must be acceptable to the source.
    assert!(ok(
        &doc_fun(vec![Some(LT::Number)], LT::Nil),
        &doc_fun(vec![Some(LT::Number)], LT::Nil)
    ));
    assert!(!ok(
        &doc_fun(vec![Some(LT::Number)], LT::Nil),
        &doc_fun(vec![Some(LT::String)], LT::Nil)
    ));
    // Parameter count is lenient: a source with fewer parameters is accepted (extra args are ignored).
    assert!(ok(
        &doc_fun(vec![], LT::Nil),
        &doc_fun(vec![Some(LT::Number)], LT::Nil)
    ));
    // Function → Function primitive.
    assert!(ok(&doc_fun(vec![], LT::Nil), &LT::Function));
}

#[test]
fn test_ref_inheritance() {
    // Inheritance chain: with `---@class Bar : Foo`, Bar ≤ Foo.
    use lsp_types::Uri;
    use std::str::FromStr;

    let emmyrc = Arc::new(crate::Emmyrc::default());
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri = Uri::from_str("file:///C:/ws/inherit.lua").unwrap();
    let fid = db.set_file_content(
        &uri,
        Some("---@class Foo\nlocal Foo = {}\n---@class Bar : Foo\nlocal Bar = {}".to_string()),
    );
    let model = crate::semantic_model::SemanticModel::new(&db, fid).expect("model");
    let foo = LuaType::Ref(LuaTypeDeclId::global("Foo"));
    let bar = LuaType::Ref(LuaTypeDeclId::global("Bar"));
    assert!(
        super::is_compatible(&model, &bar, &foo),
        "Bar ≤ Foo（继承）"
    );
    assert!(
        super::is_compatible(&model, &foo, &bar),
        "名义双向（旧语义）"
    );
    assert!(super::is_compatible(&model, &foo, &foo));
}

#[test]
fn test_alias_nominal() {
    use lsp_types::Uri;
    use std::str::FromStr;

    let emmyrc = Arc::new(crate::Emmyrc::default());
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri = Uri::from_str("file:///C:/ws/alias.lua").unwrap();
    let fid = db.set_file_content(
        &uri,
        Some("---@alias MyStr string\nlocal s = 'x'".to_string()),
    );
    let model = crate::semantic_model::SemanticModel::new(&db, fid).expect("model");
    let my_str = LuaType::Ref(LuaTypeDeclId::global("MyStr"));
    // salsa has no alias origin: nominal (same id) passes, structural expansion is degraded.
    assert!(super::is_compatible(&model, &my_str, &my_str));
}

#[test]
fn test_generic_params() {
    use LuaType as LT;
    let box_num_src = LT::Generic(Arc::new(crate::LuaGenericType::new(
        LuaTypeDeclId::global("Box"),
        vec![LT::Number],
    )));
    let box_num_tgt = LT::Generic(Arc::new(crate::LuaGenericType::new(
        LuaTypeDeclId::global("Box"),
        vec![LT::Number],
    )));
    let box_str = LT::Generic(Arc::new(crate::LuaGenericType::new(
        LuaTypeDeclId::global("Box"),
        vec![LT::String],
    )));
    assert!(ok(&box_num_src, &box_num_tgt));
    assert!(!ok(&box_num_src, &box_str));
}

#[test]
fn test_instance_base() {
    use LuaType as LT;
    let inst = LT::Instance(Arc::new(crate::LuaInstanceType::new(
        LT::Number,
        crate::InFiled::new(crate::FileId::new(0), Default::default()),
    )));
    assert!(ok(&inst, &LT::Integer));
    assert!(!ok(&inst, &LT::String));
}

#[test]
fn test_detail_reason() {
    use crate::LuaType as LT;
    use lsp_types::Uri;
    use std::str::FromStr;

    let emmyrc = Arc::new(crate::Emmyrc::default());
    let mut db = crate::SalsaDatabase::new();
    db.update_config(emmyrc.clone());
    let uri = Uri::from_str("file:///C:/ws/x.lua").unwrap();
    let fid = db.set_file_content(&uri, Some("local x = 1".to_string()));
    let model = crate::semantic_model::SemanticModel::new(&db, fid).expect("model");
    let result = check_type_detail(&model, &LT::String, &LT::Number);
    assert!(result.is_err(), "String !≤ Number 应失败");
    match result {
        Err(crate::semantic_model::type_check::TypeCheckFailReason::TypeNotMatchWithReason(
            reason,
        )) => {
            assert!(
                reason.contains("expected"),
                "reason should contain expected: {reason}"
            );
        }
        other => panic!("expected detailed reason, got {other:?}"),
    }
}
