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
        let b_expected = ws.ty("nil");
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

    #[test]
    fn test_issue_369() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @enum myenum
            local myenum = { A = 1 }

            --- @param x myenum|{}
            function foo(x)
                if type(x) ~= 'table' then
                    a = x
                else
                    b = x
                end
            end
        "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("myenum");
        assert_eq!(a, a_expected);

        let b = ws.expr_ty("b");
        let b_expected = ws.ty("{}");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_issue_373() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @alias myalias string|string[]

            --- @param x myalias
            function foo(x)
                if type(x) == 'string' then
                    a = x
                elseif type(x) == 'table' then
                    b = x
                end
            end
        "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("string");
        assert_eq!(a, a_expected);

        let b = ws.expr_ty("b");
        let b_expected = ws.ty("string[]");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_call_cast() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"

            ---@return boolean
            ---@return_cast n integer
            local function isInteger(n)
                return true
            end
            
            local a ---@type integer | string
            
            if isInteger(a) then
                d = a
            else
                e = a
            end 
            
        "#,
        );

        let d = ws.expr_ty("d");
        let d_expected = ws.ty("integer");
        assert_eq!(d, d_expected);

        let e = ws.expr_ty("e");
        let e_expected = ws.ty("string");
        assert_eq!(e, e_expected);
    }

    #[test]
    fn test_call_cast2() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"

        ---@class My2

        ---@class My1

        ---@class My3:My2,My1
        local m = {}


        ---@return boolean
        ---@return_cast self My1
        function m:isMy1()
        end

        ---@return boolean
        ---@return_cast self My2
        function m:isMy2()
        end

        if m:isMy1() then
            a = m
        elseif m:isMy2() then
            b = m
        end
        "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("My1");
        assert_eq!(a, a_expected);

        let b = ws.expr_ty("b");
        let b_expected = ws.ty("My2");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_issue_423() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        --- @return string?
        local function bar() end

        --- @param a? string
        function foo(a)
        if not a then
            a = bar()
            assert(a)
        end

        --- @type string
        local _ = a -- incorrect error
        end
        "#,
        ));
    }

    #[test]
    fn test_issue_472() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::UnnecessaryIf,
            r#"
            worldLightLevel = 0
            worldLightColor = 0
            Gmae = {}
            ---@param color integer
            ---@param level integer
            function Game.setWorldLight(color, level)
                local previousColor = worldLightColor
                local previousLevel = worldLightLevel

                worldLightColor = color
                worldLightLevel = level

                if worldLightColor ~= previousColor or worldLightLevel ~= previousLevel then
                    -- Do something...
                end
            end
            "#
        ))
    }

    #[test]
    fn test_issue_478() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            --- @param line string
            --- @param b boolean
            --- @return string
            function foo(line, b)
                return b and line or line
            end
            "#
        ));
    }

    #[test]
    fn test_issue_491() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
            ---@param srow integer?
            function foo(srow)
                srow = srow or 0

                return function()
                    ---@return integer
                    return function()
                        return srow
                    end
                end
            end
            "#
        ));
    }

    #[test]
    fn test_issue_288() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
                --- @alias MyFun fun(): string[]
                local f --- @type MyFun

                if type(f) == 'function' then
                     _, res = pcall(f)
                end
            "#,
        );

        let res = ws.expr_ty("res");
        let expected_ty = ws.ty("string|string[]");
        assert_eq!(res, expected_ty);
    }

    #[test]
    fn test_issue_480() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.check_code_for(
            DiagnosticCode::UnnecessaryAssert,
            r#"
            --- @param a integer?
            --- @param c boolean
            function foo(a, c)
                if c then
                    a = 1
                end

                assert(a)
            end
            "#,
        );
    }

    #[test]
    fn test_issue_526() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @alias A { kind: 'A'}
            --- @alias B { kind: 'B'}

            local x --- @type A|B

            if x.kind == 'A' then
                a = x
                return
            end

            b = x
            "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("A");
        assert_eq!(a, a_expected);
        let b = ws.expr_ty("b");
        let b_expected = ws.ty("B");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_issue_583() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            --- @param sha string
            local function get_hash_color(sha)
            local r, g, b = sha:match('(%x)%x(%x)%x(%x)')
            assert(r and g and b, 'Invalid hash color')
            local _ = r --- @type string
            local _ = g --- @type string
            local _ = b --- @type string
            end
            "#,
        );
    }

    #[test]
    fn test_issue_584() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local function foo()
                for _ in ipairs({}) do
                    break
                end

                local a
                if a == nil then
                    a = 1
                    local _ = a --- @type integer
                end
            end
            "#,
        );
    }

    #[test]
    fn test_feature_inherit_flow_from_const_local() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
            local ret --- @type string | nil

            local h = type(ret) == "string"
            if h then
                a = ret
            end

            local e = type(ret)
            if e == "string" then
                b = ret
            end
            "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("string");
        assert_eq!(a, a_expected);
        let b = ws.expr_ty("b");
        let b_expected = ws.ty("string");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_feature_generic_type_guard() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T
            ---@param type `T`
            ---@return TypeGuard<T>
            local function instanceOf(inst, type)
                return true
            end
            
            local ret --- @type string | nil
            
            if instanceOf(ret, "string") then
                a = ret
            end
            "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("string");
        assert_eq!(a, a_expected);
    }

    #[test]
    fn test_issue_598() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class A<T>
            A = {}
            ---@class IDisposable
            ---@class B<T>: IDisposable

            ---@class AnonymousObserver<T>: IDisposable

            ---@generic T
            ---@return AnonymousObserver<T>
            function createAnonymousObserver()
            end
            "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@param observer fun(value: T) | B<T>
                ---@return IDisposable
                function A:subscribe(observer)
                    local typ = type(observer)
                    if typ == 'function' then
                        ---@cast observer fun(value: T)
                        observer = createAnonymousObserver()
                    elseif typ == 'table' then
                        ---@cast observer -function
                        observer = createAnonymousObserver()
                    end

                    return observer
                end
            "#,
        ));

        assert!(!ws.check_code_for(
            DiagnosticCode::ReturnTypeMismatch,
            r#"
                ---@param observer fun(value: T) | B<T>
                ---@return IDisposable
                function A:test2(observer)
                    local typ = type(observer)
                    if typ == 'table' then
                        ---@cast observer -function
                        observer = createAnonymousObserver()
                    end

                    return observer
                end
            "#,
        ));
    }

    #[test]
    fn test_issue_524() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type string[]
            local d = {}

            if #d == 2 then
                a = d[1]
                b = d[2]
                c = d[3]
            end

            for i = 1, #d do
                e = d[i]
            end
            "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("string");
        assert_eq!(a, a_expected);
        let b = ws.expr_ty("b");
        let b_expected = ws.ty("string");
        assert_eq!(b, b_expected);
        let c = ws.expr_ty("c");
        let c_expected = ws.ty("string?");
        assert_eq!(c, c_expected);
        let e = ws.expr_ty("e");
        let e_expected = ws.ty("string");
        assert_eq!(e, e_expected);
    }

    #[test]
    fn test_issue_600() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@class Test2
            ---@field test string[]
            ---@field test2? string
            local a = {}
            if a.test[1] and a.test[1].char(123) then

            end
            "#,
        ));
    }

    #[test]
    fn test_issue_585() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local a --- @type type?

            if type(a) == 'string' then
                local _ = a --- @type type
            end
            "#,
        ));
    }

    #[test]
    fn test_issue_627() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class A
            ---@field type "point"
            ---@field handle number

            ---@class B
            ---@field type "unit"
            ---@field handle string

            ---@param a number
            function testA(a)
            end
            ---@param a string
            function testB(a)
            end
            "#,
        );
        assert!(ws.check_code_for(
            DiagnosticCode::ParamTypeNotMatch,
            r#"
                ---@param target A | B
                function test(target)
                    if target.type == 'point' then
                        testA(target.handle)
                    end
                    if target.type == 'unit' then
                        testB(target.handle)
                    end
                end
            "#,
        ));
    }

    #[test]
    fn test_issue_622() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Test.A
            ---@field base number
            ---@field add number
            T = {}

            ---@enum Test.op
            Op = {
                base = "base",
                add = "add",
            };
            "#,
        );
        ws.def(
            r#"
            ---@param op Test.op
            ---@param value number
            ---@return boolean
            function T:SetValue(op, value)
                local oldValue = self[op]
                if oldValue == value then
                    return false
                end
                A = oldValue
                return true
            end
            "#,
        );
        let a = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(a), "number");
    }

    #[test]
    fn test_nil_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type number?
            local angle

            if angle ~= nil and angle >= 0 then
                A = angle
            end

            "#,
        );
        let a = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(a), "number");
    }

    #[test]
    fn test_type_narrow() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@generic T: table
            ---@param obj T | function
            ---@return T?
            function bindGC(obj)
                if type(obj) == 'table' then
                    A = obj
                end
            end
            "#,
        );
        let a = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(a), "T");
    }

    #[test]
    fn test_issue_630() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class A
            ---@field Abc string?
            A = {}
            "#,
        );
        ws.def(
            r#"
            function A:test()
                if not rawget(self, 'Abc') then
                    self.Abc = "a"
                end

                B = self.Abc
                C = self
            end
            "#,
        );
        let a = ws.expr_ty("B");
        assert_eq!(ws.humanize_type(a), "string");
        let c = ws.expr_ty("C");
        assert_eq!(ws.humanize_type(c), "A");
    }

    #[test]
    fn test_error_function() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
                ---@class Result
                ---@field value string?
                Result = {}

                function getValue()
                    ---@type Result?
                    local result

                    if result then
                        error(result.value)
                    end
                end
            "#,
        ));
    }

    #[test]
    fn test_array_flow() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::NeedCheckNil,
            r#"
            for i = 1, #_G.arg do
                print(_G.arg[i].char())
            end
            "#,
        ));
    }

    #[test]
    fn test_issue_641() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local b --- @type boolean
            local tar = b and 'a' or 'b'

            if tar == 'a' then
            end

            --- @type 'a'|'b'
            local _ = tar
            "#,
        ));
    }

    #[test]
    fn test_self_1() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Node
            ---@field parent? Node 

            ---@class Subject<T>: Node
            ---@field package root? Node 
            Subject = {}
            "#,
        );
        ws.def(
            r#"
            function Subject:add()
                if self == self.parent then
                    A = self
                end
            end
            "#,
        );
        let a = ws.expr_ty("A");
        assert_eq!(ws.humanize_type(a), "Node");
    }
}
