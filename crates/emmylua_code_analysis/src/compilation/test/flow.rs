#[cfg(test)]
mod test {

    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    #[test]
    fn test_closure_return() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        --- @generic T, U
        --- @param arr T[]
        --- @param op fun(item: T, index: integer): U
        --- @return U[]
        function map(arr, op)
        end
        "#,
        );

        let ty = ws.expr_ty(
            r#"
        map({ 1, 2, 3 }, function(item, i)
            return tostring(item)
        end)
        "#,
        );
        let expected = ws.ty("string[]");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_140_1() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        ---@class Object
        
        ---@class T
        local inject2class ---@type (Object| T)?
        if jsonClass then
            if inject2class then
                A = inject2class
            end
        end
        "#,
        );

        let ty = ws.expr_ty("A");
        let type_desc = ws.humanize_type(ty);
        assert_eq!(type_desc, "T");
    }

    #[test]
    fn test_issue_140_2() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        local msgBody ---@type { _hgQuiteMsg : 1 }?
        if not msgBody or not msgBody._hgQuiteMsg then
        end
        "#
        ));
    }

    #[test]
    fn test_issue_140_3() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        local SELF ---@type unknown
        if SELF ~= nil then
            SELF:OnDestroy()
        end
        "#
        ));
    }

    #[test]
    fn test_issue_107() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type {bar?: fun():string}
        local props
        if props.bar then
            local foo = props.bar()
        end

        if type(props.bar) == 'function' then
            local foo = props.bar() 
        end

        local foo = props.bar and props.bar() or nil 
        "#
        ));
    }

    #[test]
    fn test_issue_100() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        local f = io.open('', 'wb')
        if not f then
            error("Could not open a file")
        end

        f:write('')
        "#
        ));
    }

    #[test]
    fn test_issue_93() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        local text    --- @type string[]?
        if staged then
            local text1 --- @type string[]?
            text = text1
        else
            local text2 --- @type string[]?
            text = text2
        end

        if not text then
            return
        end

        --- @param _a string[]
        local function foo(_a) end

        foo(text)
        "#
        ));
    }

    #[test]
    fn test_null_function_field() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@class A
        ---@field aaa? fun(a: string)


        local c ---@type A

        if c.aaa then
            c.aaa("aaa")
        end
        "#
        ))
    }

    #[test]
    fn test_issue_162() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
            --- @class Foo
            --- @field a? fun()

            --- @param _o Foo
            function bar(_o) end

            bar({})
            "#
        ));
    }

    #[test]
    fn test_redefine() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::UndefinedField,
            r#"
            ---@class AA
            ---@field b string

            local a = 1
            a = 1

            ---@type AA
            local a

            print(a.b)
            "#
        ));
    }

    #[test]
    fn test_issue_165() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
local a --- @type table?
if not a or #a == 0 then
    return
end

print(a.h)
            "#
        ));
    }

    #[test]
    fn test_issue_160() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
local a --- @type table?

if not a then
    assert(a)
end

print(a.field)
            "#
        ));
    }

    #[test]
    fn test_issue_210() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
        --- @class A
        --- @field b integer

        local a = {}

        --- @type A
        a = { b = 1 }

        --- @param _a A
        local function foo(_a) end

        foo(a)
        "#
        ));
    }

    #[test]
    fn test_issue_224() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
        --- @class A

        --- @param opts? A
        --- @return A
        function foo(opts)
            opts = opts or {}
            return opts
        end
        "#
        ));
    }

    #[test]
    fn test_elseif() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
---@class D11
---@field public a string

---@type D11|nil
local a

if not a then
elseif a.a then
    print(a.a)
end

        "#
        ));
    }

    #[test]
    fn test_issue_266() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        --- @return string
        function baz() end

        local a
        a = baz() -- a has type nil but should be string    
        d = a    
        "#
        ));

        let d = ws.expr_ty("d");
        let d_desc = ws.humanize_type(d);
        assert_eq!(d_desc, "string");
    }

    #[test]
    fn test_issue_277() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@param t? table
        function myfun3(t)
            if type(t) ~= 'table' then
                return
            end

            a = t
        end   
        "#,
        );

        let a = ws.expr_ty("a");
        let a_desc = ws.humanize_type(a);
        assert_eq!(a_desc, "table");
    }

    #[test]
    fn test_docint() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local stack = 0
            if stack ~= 0 then
                a = stack
            end
        "#,
        );

        let a = ws.expr_ty("a");
        let a_desc = ws.humanize_type(a);
        assert_eq!(a_desc, "integer");
    }

    #[test]
    fn test_issue_147() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local d ---@type string?
            if d then
                local d2 = function(...)
                    e = d
                end
            end

        "#,
        );

        let e = ws.expr_ty("e");
        assert_eq!(e, LuaType::String);
    }

    #[test]
    fn test_issue_325() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        while condition do
            local a ---@type string?
            if not a then
                break
            end
            b = a
        end

        "#,
        );

        let b = ws.expr_ty("b");
        assert_eq!(b, LuaType::String);
    }

    #[test]
    fn test_issue_347() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
        --- @param x 'a'|'b'
        --- @return 'a'|'b'
        function foo(x)
        if x ~= 'a' and x ~= 'b' then
            error('invalid behavior')
        end

        return x
        end
        "#,
        ));
    }

    #[test]
    fn test_issue_339() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        --- @class A

        local a --- @type A|string

        if type(a) == 'table' then
            b = a -- a should be A
        else
            c = a -- a should be string
        end
        "#,
        );

        let b = ws.expr_ty("b");
        let b_expected = ws.ty("A");
        assert_eq!(b, b_expected);

        let c = ws.expr_ty("c");
        let c_expected = ws.ty("string");
        assert_eq!(c, c_expected);
    }

    #[test]
    fn test_unknown_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local a
        b = a
        "#,
        );

        let b = ws.expr_ty("b");
        let b_expected = ws.ty("unknown");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_issue_367() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local files
        local function init()
            if files then
                return
            end
            files = {}
            a = files -- a 与 files 现在均为 nil
        end
        "#,
        );

        let a = ws.expr_ty("a");
        assert!(a != LuaType::Nil);

        ws.def(
            r#"
            ---@alias D10.data
            ---| number
            ---| string
            ---| boolean
            ---| table
            ---| nil

            ---@param data D10.data
            local function init(data)
                ---@cast data table
                
                b = data -- data 现在仍为 `10.data` 而不是 `table`
            end
            "#,
        );

        let b = ws.expr_ty("b");
        let b_desc = ws.humanize_type(b);
        assert_eq!(b_desc, "table");
    }

    #[test]
    fn test_issue_364() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@param k integer
            ---@param t table<integer,integer>
            function foo(k, t)
                if t and t[k] then
                    return t[k]
                end

                if t then
                    -- t is nil -- incorrect
                    t[k] = 1 -- t may be nil -- incorrect
                end
            end
            "#,
        ));
    }

    #[test]
    fn test_issue_382() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@class Trigger

            ---@class Event
            ---@field private wait_pushing? Trigger[]
            local M


            ---@param trigger Trigger
            function M:add_trigger(trigger)
                if not self.wait_pushing then
                    self.wait_pushing = {}
                end
                self.wait_pushing[1] = trigger
            end

            ---@private
            function M:check_waiting()
                if self.wait_pushing then
                end
            end
            "#,
        ));
    }
}
