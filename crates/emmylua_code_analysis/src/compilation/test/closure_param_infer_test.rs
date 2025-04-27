#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_closure_param_infer() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"

        ---@class EventData
        ---@field name string

        ---@class EventDispatcher
        ---@field pre fun(self:EventDispatcher,callback:fun(context:EventData))
        local EventDispatcher = {}

        EventDispatcher:pre(function(context)
            b = context
        end)
        "#,
        );

        let ty = ws.expr_ty("b");
        let expected = ws.ty("EventData");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_function_param_inherit() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@alias Outfit_t table

        ---@class Creature
        ---@field onChangeOutfit fun(self:Creature, outfit:Outfit_t):boolean
        ---@overload fun(id:integer):Creature?
        Creature = {}

        function Creature:onChangeOutfit(outfit)
            a = outfit
        end
 
        "#,
        );

        let ty = ws.expr_ty("a");
        let expected = ws.ty("Outfit_t");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_table_field_function_param() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias ProxyHandler.Getter fun(self: self, raw: any, key: any, receiver: table): any
            
            ---@class ProxyHandler
            ---@field get ProxyHandler.Getter
        "#,
        );

        ws.def(
            r#"

        ---@class A: ProxyHandler
        local A

        function A:get(target, key, receiver, name)
            a = self
        end
                "#,
        );
        let ty = ws.expr_ty("a");
        let expected = ws.ty("A");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));

        ws.def(
            r#"

        ---@class B: ProxyHandler
        local B

        B.get = function(self, target, key, receiver, name)
            b = self
        end
                "#,
        );
        let ty = ws.expr_ty("b");
        let expected = ws.ty("B");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));

        ws.def(
            r#"
        ---@class C: ProxyHandler
        local C = {
            get = function(self, target, key, receiver, name)
                c = self
            end,
        }
                "#,
        );
        let ty = ws.expr_ty("c");
        let expected = ws.ty("C");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_table_field_function_param_2() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class ProxyHandler
            local P

            ---@param raw any
            ---@param key any
            ---@param receiver table
            ---@return any
            function P:get(raw, key, receiver) end
            "#,
        );

        ws.def(
            r#"
            ---@class A: ProxyHandler
            local A

            function A:get(raw, key, receiver)
                a = receiver
            end
            "#,
        );
        let ty = ws.expr_ty("a");
        let expected = ws.ty("table");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_table_field_function_param_3() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class SimpleClass.Meta
            ---@field __defineSet fun(self: self, key: string, f: fun(self: self, value: any))

            ---@class Dep:  SimpleClass.Meta
            local Dep
            Dep:__defineSet('subs', function(self, value)
                a  = self          
            end)
            "#,
        );
        let ty = ws.expr_ty("a");
        let expected = ws.ty("Dep");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_table_field_function_param_4() {
        let mut ws = VirtualWorkspace::new();
        ws.def(r#"
                ---@alias ProxyHandler.Getter fun(self: self, raw: any, key: any, receiver: table): any

                ---@class ProxyHandler
                ---@field get? ProxyHandler.Getter
            "#
        );

        ws.def(
            r#"
            ---@class ShallowUnwrapHandlers: ProxyHandler
            local ShallowUnwrapHandlers = {
                get = function(self, target, key, receiver)
                    a = self
                end,
            }
            "#,
        );
        let ty = ws.expr_ty("a");
        let expected = ws.ty("ShallowUnwrapHandlers");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_issue_350() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                --- @param x string|fun(args: string[])
                function cmd(x) end
            "#,
        );

        ws.def(
            r#"
                cmd(function(args)
                a = args -- should be string[]
                end)
            "#,
        );
        let ty = ws.expr_ty("a");
        let expected = ws.ty("string[]");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_field_doc_function() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class ClosureTest
            ---@field e fun(a: string, b: number)
            ---@field e fun(a: number, b: number)
            local Test

            function Test.e(a, b)
                A = a
            end
            "#,
        );
        let ty = ws.expr_ty("A");
        let expected = ws.ty("string|number");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_field_doc_function_2() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class ClosureTest
            ---@field e fun(a: string, b: number)
            ---@field e fun(a: number, b: number)
            local Test

            ---@overload fun(a: string, b: number)
            ---@overload fun(a: number, b: number)
            function Test.e(a, b)
                d = b
            end
            "#,
        );
        let ty = ws.expr_ty("d");
        let expected = ws.ty("number");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_field_doc_function_3() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class ClosureTest
            ---@field e fun(a: string, b: number) -- 不在 overload 时必须声明 self 才被视为方法
            ---@field e fun(a: number, b: number)
            local Test

            function Test:e(a, b) -- `:`声明
                A = a
            end
            "#,
        );
        let ty = ws.expr_ty("A");
        let expected = ws.ty("number");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }
}
