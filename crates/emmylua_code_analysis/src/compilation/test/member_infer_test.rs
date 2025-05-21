#[cfg(test)]
mod test {
    use smol_str::SmolStr;

    use crate::{LuaType, LuaUnionType, VirtualWorkspace};

    #[test]
    fn test_issue_318() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local map = {
            a = 'a',
            b = 'b',
            c = 'c',
        }
        local key      --- @type string
        c = map[key]   -- type should be ('a'|'b'|'c'|nil)

        "#,
        );

        let c_ty = ws.expr_ty("c");

        let union_type = LuaType::Union(
            LuaUnionType::new(vec![
                LuaType::StringConst(SmolStr::new("a").into()),
                LuaType::StringConst(SmolStr::new("b").into()),
                LuaType::StringConst(SmolStr::new("c").into()),
                LuaType::Nil,
            ])
            .into(),
        );

        assert_eq!(c_ty, union_type);
    }

    #[test]
    fn test_issue_314_generic_inheritance() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@class foo<T>: T
        local foo_mt = {}

        ---@type foo<{a: string}>
        local bar = { a = 'test' }

        c = bar.a -- should be string

        ---@class buz<T>: foo<T>
        local buz_mt = {}

        ---@type buz<{a: integer}>
        local qux = { a = 5 }

        d = qux.a -- should be integer
        "#,
        );

        let c_ty = ws.expr_ty("c");
        let d_ty = ws.expr_ty("d");

        assert_eq!(c_ty, LuaType::String);
        assert_eq!(d_ty, LuaType::Integer);
    }

    #[test]
    fn test_issue_397() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @class A
        --- @field field? integer

        --- @class B : A
        --- @field field integer

        --- @type B
        local b = { field = 1 }

        local key1 --- @type 'field'
        local key2 = 'field'

        a = b.field -- type is integer - correct
        d = b['field'] -- type is integer - correct
        e = b[key1] -- type is integer? - wrong
        f = b[key2] -- type is integer? - wrong
        "#,
        );

        let a_ty = ws.expr_ty("a");
        let d_ty = ws.expr_ty("d");
        let e_ty = ws.expr_ty("e");
        let f_ty = ws.expr_ty("f");

        assert_eq!(a_ty, LuaType::Integer);
        assert_eq!(d_ty, LuaType::Integer);
        assert_eq!(e_ty, LuaType::Integer);
        assert_eq!(f_ty, LuaType::Integer);
    }

    #[test]
    fn test_issue_454_unions() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @class A
        --- @field a integer
        --- @field c integer
        --- @field d integer

        --- @class B
        --- @field b integer
        --- @field c integer
        --- @field d string

        ab --- @type A | B
        "#,
        );

        assert_eq!(ws.expr_ty("ab.a"), LuaType::Unknown);
        assert_eq!(ws.expr_ty("ab.b"), LuaType::Unknown);
        assert_eq!(ws.expr_ty("ab.c"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab.d"), ws.ty("integer | string"));
        assert_eq!(ws.expr_ty("ab['a']"), LuaType::Unknown);
        assert_eq!(ws.expr_ty("ab['b']"), LuaType::Unknown);
        assert_eq!(ws.expr_ty("ab['c']"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab['d']"), ws.ty("integer | string"));
    }

    #[test]
    fn test_issue_454_intersections() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @class A
        --- @field a integer
        --- @field c integer
        --- @field d integer

        --- @class B
        --- @field b integer
        --- @field c integer
        --- @field d string

        ab --- @type A & B
        "#,
        );

        assert_eq!(ws.expr_ty("ab.a"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab.b"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab.c"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab.d"), ws.ty("integer & string"));
        assert_eq!(ws.expr_ty("ab['a']"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab['b']"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab['c']"), ws.ty("integer"));
        assert_eq!(ws.expr_ty("ab['d']"), ws.ty("integer & string"));
    }
}
