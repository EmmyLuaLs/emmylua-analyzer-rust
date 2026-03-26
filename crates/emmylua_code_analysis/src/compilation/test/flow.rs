#[cfg(test)]
mod test {
    use crate::{DiagnosticCode, LuaType, VirtualWorkspace};

    const STACKED_TYPE_GUARDS: usize = 180;
    const LARGE_LINEAR_ASSIGNMENT_STEPS: usize = 2048;

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
    fn test_stacked_same_var_type_guards_build_semantic_model() {
        let mut ws = VirtualWorkspace::new();
        let repeated_guards =
            "if type(value) ~= 'string' then return end\n".repeat(STACKED_TYPE_GUARDS);
        let block = format!(
            r#"
        local value ---@type string|integer|boolean

        {repeated_guards}
        local narrowed ---@type string
        narrowed = value
        "#,
        );

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for stacked same-variable type guard repro"
        );
        assert!(ws.check_code_for(DiagnosticCode::AssignTypeMismatch, &block));
    }

    #[test]
    fn test_stacked_same_var_call_type_guards_build_semantic_model() {
        let mut ws = VirtualWorkspace::new();
        let repeated_guards =
            "if not instance_of(value, 'string') then return end\n".repeat(STACKED_TYPE_GUARDS);
        let block = format!(
            r#"
        ---@generic T
        ---@param inst any
        ---@param type `T`
        ---@return TypeGuard<T>
        local function instance_of(inst, type)
            return true
        end

        local value ---@type string|integer|boolean

        {repeated_guards}
        after_guard = value
        "#,
        );

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for stacked same-variable call type guard repro"
        );
        assert_eq!(ws.expr_ty("after_guard"), ws.ty("string"));
    }

    #[test]
    fn test_stacked_same_var_call_type_guard_eq_false_build_semantic_model() {
        let mut ws = VirtualWorkspace::new();
        let repeated_guards = "if instance_of(value, 'string') == false then return end\n"
            .repeat(STACKED_TYPE_GUARDS);
        let block = format!(
            r#"
        ---@generic T
        ---@param inst any
        ---@param type `T`
        ---@return TypeGuard<T>
        local function instance_of(inst, type)
            return true
        end

        local value ---@type string|integer|boolean

        {repeated_guards}
        after_guard = value
        "#,
        );

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for stacked binary call type guard repro"
        );
        assert_eq!(ws.expr_ty("after_guard"), ws.ty("string"));
    }

    #[test]
    fn test_branch_join_keeps_union_when_only_one_side_narrows() {
        let mut ws = VirtualWorkspace::new();
        let block = r#"
        local cond ---@type boolean
        local value ---@type string|integer

        if cond then
            if type(value) ~= 'string' then
                return
            end
        end

        after_join = value
        "#;

        let file_id = ws.def(block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for branch join merge-safety repro"
        );
        assert_eq!(ws.expr_ty("after_join"), ws.ty("string|integer"));
    }

    #[test]
    fn test_stacked_same_field_truthiness_guards_build_semantic_model() {
        let mut ws = VirtualWorkspace::new();
        let repeated_guards = "if not value.foo then return end\n".repeat(STACKED_TYPE_GUARDS);
        let block = format!(
            r#"
        ---@class HasFoo
        ---@field foo string

        ---@class NoFoo
        ---@field bar integer

        local value ---@type HasFoo|NoFoo

        {repeated_guards}
        after_guard = value
        "#,
        );

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for stacked same-field truthiness repro"
        );
        assert_eq!(ws.expr_ty("after_guard"), ws.ty("HasFoo"));
    }

    #[test]
    fn test_stacked_return_cast_guards_build_semantic_model() {
        let mut ws = VirtualWorkspace::new();
        let repeated_guards =
            "if not is_player(creature) then return end\n".repeat(STACKED_TYPE_GUARDS);
        let block = format!(
            r#"
        ---@class Creature

        ---@class Player: Creature

        ---@class Monster: Creature

        ---@return boolean
        ---@return_cast creature Player else Monster
        local function is_player(creature)
            return true
        end

        local creature ---@type Creature

        {repeated_guards}
        after_guard = creature
        "#,
        );

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for stacked return-cast repro"
        );
        assert_eq!(ws.expr_ty("after_guard"), ws.ty("Player"));
    }

    #[test]
    fn test_stacked_return_cast_self_guards_build_semantic_model() {
        let mut ws = VirtualWorkspace::new();
        let repeated_guards =
            "if not creature:is_player() then return end\n".repeat(STACKED_TYPE_GUARDS);
        let block = format!(
            r#"
        ---@class Creature

        ---@class Player: Creature

        ---@class Monster: Creature
        local creature = {{}}

        ---@return boolean
        ---@return_cast self Player else Monster
        function creature:is_player()
            return true
        end

        {repeated_guards}
        after_guard = creature
        "#,
        );

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for stacked self return-cast repro"
        );
        assert_eq!(ws.expr_ty("after_guard"), ws.ty("Player"));
    }

    #[test]
    fn test_large_linear_assignment_file_builds_semantic_model() {
        let mut ws = VirtualWorkspace::new();
        let mut block = String::from(
            r#"
        local value ---@type integer
        value = 1

        "#,
        );

        for i in 0..LARGE_LINEAR_ASSIGNMENT_STEPS {
            block.push_str(&format!("local alias_{i} = value\n"));
            block.push_str(&format!("value = alias_{i}\n"));
        }
        block.push_str("after_assign = value\n");

        let file_id = ws.def(&block);

        assert!(
            ws.analysis
                .compilation
                .get_semantic_model(file_id)
                .is_some(),
            "expected semantic model for large linear assignment stress case"
        );
        let after_assign = ws.expr_ty("after_assign");
        assert_eq!(ws.humanize_type(after_assign), "integer");
    }

    #[test]
    fn test_pending_replay_order_uses_type_guard_before_self_return_cast_lookup() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Player

        ---@class Monster

        local checker = {}

        ---@return boolean
        ---@return_cast self Player else Monster
        function checker:is_player()
            return true
        end

        local branch ---@type boolean
        local creature = branch and checker or false

        if type(creature) ~= "table" then
            return
        end

        if not creature:is_player() then
            return
        end

        after_guard = creature
        "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.ty("Player"));
    }

    #[test]
    fn test_pending_replay_order_with_three_guards_before_self_lookup() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class PlayerA

        ---@class MonsterA

        ---@class PlayerB

        ---@class MonsterB

        local checker_a = {
            kind = "checker_a",
        }

        ---@return boolean
        ---@return_cast self PlayerA else MonsterA
        function checker_a:is_player()
            return true
        end

        local checker_b = {
            kind = "checker_b",
        }

        ---@return boolean
        ---@return_cast self PlayerB else MonsterB
        function checker_b:is_player()
            return true
        end

        local allow_false ---@type boolean
        local choose_a ---@type boolean
        local creature = allow_false and false or (choose_a and checker_a or checker_b)

        if type(creature) ~= "table" then
            return
        end

        if creature.kind ~= "checker_a" then
            return
        end

        if creature:is_player() == false then
            return
        end

        after_guard = creature
        "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.ty("PlayerA"));
    }

    #[test]
    fn test_return_cast_self_guard_uses_prior_narrowing_for_method_lookup() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Player

        ---@class Monster

        local checker = {
            kind = "checker",
        }

        ---@return boolean
        ---@return_cast self Player else Monster
        function checker:is_player()
            return true
        end

        local monster = {
            kind = "monster",
        }

        local branch ---@type boolean
        local creature = branch and checker or monster

        if creature.kind ~= "checker" then
            return
        end

        if not creature:is_player() then
            return
        end

        after_guard = creature
        "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.ty("Player"));
    }

    #[test]
    fn test_return_cast_self_guard_without_prior_method_lookup_narrowing_does_not_apply() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Player

        ---@class Monster

        local checker = {
            kind = "checker",
        }

        ---@return boolean
        ---@return_cast self Player else Monster
        function checker:is_player()
            return true
        end

        local monster = {
            kind = "monster",
        }

        local branch ---@type boolean
        local creature = branch and checker or monster
        before_guard = creature

        if not creature:is_player() then
            return
        end

        after_guard = creature
        "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.expr_ty("before_guard"));
    }

    #[test]
    fn test_return_cast_self_guard_with_multiple_method_candidates_uses_prior_narrowing() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class PlayerA

        ---@class MonsterA

        ---@class PlayerB

        ---@class MonsterB

        local checker_a = {
            kind = "checker_a",
        }

        ---@return boolean
        ---@return_cast self PlayerA else MonsterA
        function checker_a:is_player()
            return true
        end

        local checker_b = {
            kind = "checker_b",
        }

        ---@return boolean
        ---@return_cast self PlayerB else MonsterB
        function checker_b:is_player()
            return true
        end

        local branch ---@type boolean
        local creature = branch and checker_a or checker_b

        if creature.kind ~= "checker_a" then
            return
        end

        if not creature:is_player() then
            return
        end

        after_guard = creature
        "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.ty("PlayerA"));
    }

    #[test]
    fn test_return_cast_self_guard_with_non_callable_member_uses_prior_narrowing() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Player

        ---@class Monster

        local checker = {
            kind = "checker",
        }

        ---@return boolean
        ---@return_cast self Player else Monster
        function checker:is_player()
            return true
        end

        local monster = {
            kind = "monster",
            is_player = false,
        }

        local branch ---@type boolean
        local creature = branch and checker or monster

        if creature.kind ~= "checker" then
            return
        end

        if not creature:is_player() then
            return
        end

        after_guard = creature
        "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.ty("Player"));
    }

    #[test]
    fn test_return_cast_self_guard_eq_false_uses_prior_narrowing_for_method_lookup() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@class Player

        ---@class Monster

        local checker = {
            kind = "checker",
        }

        ---@return boolean
        ---@return_cast self Player else Monster
        function checker:is_player()
            return true
        end

        local monster = {
            kind = "monster",
            is_player = false,
        }

        local branch ---@type boolean
        local creature = branch and checker or monster

        if creature.kind ~= "checker" then
            return
        end

        if creature:is_player() == false then
            return
        end

        after_guard = creature
        "#,
        );

        assert_eq!(ws.expr_ty("after_guard"), ws.ty("Player"));
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
            DiagnosticCode::ParamTypeMismatch,
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
            DiagnosticCode::ParamTypeMismatch,
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
            DiagnosticCode::ParamTypeMismatch,
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
    fn test_doc_function_assignment_narrowing0() {
        let mut ws = VirtualWorkspace::new();

        let code = r#"
        local i --- @type integer|fun():string
        i = "str"
        A = i
        "#;

        ws.def(code);
        let a = ws.expr_ty("A");
        let a_desc = ws.humanize_type_detailed(a);
        assert_eq!(a_desc, "\"str\"");
    }

    #[test]
    fn test_doc_member_assignment_prefers_annotation_source() {
        let mut ws = VirtualWorkspace::new();

        let code = r#"
        local t = {}
        t.a = "hello"
        ---@type string|number
        t.a = 1
        b = t.a
        "#;

        ws.def(code);
        assert_eq!(ws.expr_ty("b"), ws.ty("integer"));
    }

    #[test]
    fn test_assignment_narrow_drops_nil_on_mismatch() {
        let mut ws = VirtualWorkspace::new();

        let code = r#"
        local a ---@type string?
        a = 1
        b = a
        "#;

        ws.def(code);
        assert_eq!(ws.expr_ty("b"), LuaType::IntegerConst(1));
    }

    #[test]
    fn test_doc_member_assignment_falls_back_to_annotation() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local t = {}
            ---@type string|number
            t.a = true
            b = t.a
        "#,
        );

        let b = ws.expr_ty("b");
        let expected_ty = ws.ty("string|number");
        let expected = ws.humanize_type(expected_ty);
        assert_eq!(ws.humanize_type(b), expected);
    }

    #[test]
    fn test_doc_function_assignment_narrowing() {
        let mut ws = VirtualWorkspace::new();

        let code = r#"
        local i --- @type integer|fun():string
        i = function() end
        _ = i()
        A = i
        "#;

        ws.def(code);

        assert!(ws.check_code_for(DiagnosticCode::CallNonCallable, code));
        assert!(ws.check_code_for(DiagnosticCode::NeedCheckNil, code));

        let a = ws.expr_ty("A");
        let a_desc = ws.humanize_type_detailed(a);
        assert_eq!(a_desc, "fun()");
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
    fn test_issue_921_or_with_empty_table() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @class Opts
            --- @field a? string

            local opts --- @type Opts?

            -- Test expression type: opts or {} should narrow to Opts
            E = opts or {}
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), "Opts");
    }

    #[test]
    fn test_issue_921_or_with_table_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local opts --- @type table?

            -- Test with plain table? type
            E = opts or {}
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), "table");
    }

    #[test]
    fn test_issue_921_self_assignment_with_table() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local opts --- @type table?

            opts = opts or {}

            E = opts
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), "table");
    }

    #[test]
    fn test_issue_921_self_assignment_with_class_empty_table() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @class Opts
            --- @field a? string

            local opts0 --- @type Opts?
            local opts1 --- @type Opts?

            opts0 = opts0 or {}
            opts1 = opts0 or { a = 'a' }

            E0 = opts0
            E1 = opts1
            "#,
        );

        // After self-assignment opts = opts or {}, opts should be narrowed to Opts
        let e0_ty = ws.expr_ty("E0");
        assert_eq!(ws.humanize_type(e0_ty), "Opts");
        let e1_ty = ws.expr_ty("E1");
        assert_eq!(ws.humanize_type(e1_ty), "Opts");
    }

    #[test]
    fn test_issue_921_and_with_string_nullable() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @class Opts
            --- @field a? string

            local opts --- @type Opts

            -- When opts.a is string?, result should be table|nil
            -- The table {'a'} is inferred as a tuple containing 'a'
            E = opts.a and { 'a' }
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), r#"("a")?"#);
    }

    #[test]
    fn test_issue_921_and_with_boolean_nullable_table() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @class Opts
            --- @field b? boolean

            local opts --- @type Opts

            -- When opts.b is boolean?, result should be false|nil|table
            E = opts.b and { 'b' }
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), r#"(false|("b"))?"#);
    }

    #[test]
    fn test_issue_921_and_with_boolean_nullable_string() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local bool --- @type boolean?

            -- When bool is boolean?, result should be false|nil|'a'
            E = bool and 'a'
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), r#"(false|"a")?"#);
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
    fn test_while_loop_post_flow_keeps_incoming_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local condition ---@type boolean
        local value ---@type string?

        while condition do
            value = "loop"
        end

        after_loop = value
        "#,
        );

        assert_eq!(ws.expr_ty("after_loop"), ws.ty("string?"));
    }

    #[test]
    fn test_repeat_loop_post_flow_keeps_body_assignment() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local condition ---@type boolean
        local value ---@type string?

        repeat
            value = "loop"
        until condition

        after_loop = value
        "#,
        );

        assert_eq!(ws.expr_ty("after_loop"), ws.ty("string"));
    }

    #[test]
    fn test_numeric_for_loop_post_flow_keeps_incoming_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local value ---@type string?

        for i = 1, 3 do
            value = "loop"
        end

        after_loop = value
        "#,
        );

        assert_eq!(ws.expr_ty("after_loop"), ws.ty("string?"));
    }

    #[test]
    fn test_for_in_loop_post_flow_keeps_incoming_type_after_break() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();

        ws.def(
            r#"
        local value ---@type string?

        for _, _value in ipairs({ "loop" }) do
            value = "loop"
            break
        end

        after_loop = value
        "#,
        );

        assert_eq!(ws.expr_ty("after_loop"), ws.ty("string?"));
    }

    #[test]
    fn test_nested_while_loop_post_flow_keeps_incoming_type() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local outer_condition ---@type boolean
        local inner_condition ---@type boolean
        local value ---@type string?

        while outer_condition do
            while inner_condition do
                value = "loop"
                break
            end

            break
        end

        after_loop = value
        "#,
        );

        assert_eq!(ws.expr_ty("after_loop"), ws.ty("string?"));
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
    fn test_narrow_after_error_branches() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        local r --- @type string?
        local a --- @type boolean
        if not r then
            if a then
                error()
            else
                error()
            end
        end

        b = r -- should be string
        "#,
        );

        let b = ws.expr_ty("b");
        assert_eq!(b, LuaType::String);
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
            DiagnosticCode::ParamTypeMismatch,
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

        // Note: we can't use `ws.ty_expr("A")` to get a true type of `A`
        // because `infer_global_type` will not allow generic variables
        // from `bindGC` to escape into global space.
        let db = &ws.analysis.compilation.db;
        let decl_id = db
            .get_global_index()
            .get_global_decl_ids("A")
            .unwrap()
            .first()
            .unwrap()
            .clone();
        let typ = db
            .get_type_index()
            .get_type_cache(&decl_id.into())
            .unwrap()
            .as_type();

        assert_eq!(ws.humanize_type(typ.clone()), "T");
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

    #[test]
    fn test_return_cast_multi_file() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "test.lua",
            r#"
            local M = {}

            --- @return boolean
            --- @return_cast _obj function
            function M.is_callable(_obj) end

            return M
            "#,
        );
        ws.def(
            r#"
            local test = require("test")

            local obj

            if test.is_callable(obj) then
                o = obj
            end
            "#,
        );
        let a = ws.expr_ty("o");
        let expected = LuaType::Function;
        assert_eq!(a, expected);
    }

    #[test]
    fn test_issue_734() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
local a --- @type string[]

assert(#a >= 1)

--- @type string
_ = a[1]

assert(#a == 1)

--- @type string
_ = a[1]

--- @type string
_2 = a[1]
            "#
        ));
    }

    #[test]
    fn test_return_cast_with_fallback() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Creature

            ---@class Player: Creature

            ---@class Monster: Creature

            ---@return boolean
            ---@return_cast creature Player else Monster
            local function isPlayer(creature)
                return true
            end

            local creature ---@type Creature

            if isPlayer(creature) then
                a = creature
            else
                b = creature
            end
            "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("Player");
        assert_eq!(a, a_expected);

        let b = ws.expr_ty("b");
        let b_expected = ws.ty("Monster");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_return_cast_with_fallback_self() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Creature

            ---@class Player: Creature

            ---@class Monster: Creature
            local m = {}

            ---@return boolean
            ---@return_cast self Player else Monster
            function m:isPlayer()
            end

            if m:isPlayer() then
                a = m
            else
                b = m
            end
            "#,
        );

        let a = ws.expr_ty("a");
        let a_expected = ws.ty("Player");
        assert_eq!(a, a_expected);

        let b = ws.expr_ty("b");
        let b_expected = ws.ty("Monster");
        assert_eq!(b, b_expected);
    }

    #[test]
    fn test_return_cast_backward_compatibility() {
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

        // Should still use the original behavior (remove integer from union)
        let e = ws.expr_ty("e");
        let e_expected = ws.ty("string");
        assert_eq!(e, e_expected);
    }

    #[test]
    fn test_issue_868() {
        let mut ws = VirtualWorkspace::new();

        ws.check_code_for(
            DiagnosticCode::AssignTypeMismatch,
            r#"
            local a --- @type string|{foo:boolean, bar:string}

            if a.foo then
                --- @type string
                local _ = a.bar
            end
            "#,
        );
    }

    #[test]
    fn test_or_empty_table_non_table_compatible() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local a --- @type string?

            -- When left type is NOT table-compatible, should not narrow
            E = a or {}
            "#,
        );

        let e_ty = ws.expr_ty("E");
        // string? or {} results in string|table (empty table becomes table)
        assert_eq!(ws.humanize_type(e_ty), "(string|table)");
    }

    #[test]
    fn test_or_empty_table_with_nonempty_class() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @class MyClass
            --- @field x number

            local obj --- @type MyClass?

            E = obj or {}
            "#,
        );

        let e_ty = ws.expr_ty("E");
        assert_eq!(ws.humanize_type(e_ty), "(MyClass|table)");
    }

    #[test]
    fn test_or_empty_table_union_of_tables() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            --- @class A
            --- @field a number

            --- @class B
            --- @field b string

            local obj --- @type (A|B)?

            -- Union of class types is table-compatible
            E = obj or {}
            "#,
        );

        let e_ty = ws.expr_ty("E");
        let type_str = ws.humanize_type_detailed(e_ty);
        assert_eq!(type_str, "(A|B|table)");
    }

    #[test]
    fn test_assignment_from_wider_single_return_call_drops_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Foo
            ---@field kind "foo"
            ---@field a integer

            ---@class Bar
            ---@field kind "bar"
            ---@field b integer

            ---@param ok boolean
            ---@return Foo|Bar
            local function pick(ok)
                if ok then
                    return { kind = "foo", a = 1 }
                end

                return { kind = "bar", b = 2 }
            end

            local ok ---@type boolean
            local x ---@type Foo|Bar

            if x.kind == "foo" then
                x = pick(ok)
                after_assign = x
            end
            "#,
        );

        assert_eq!(ws.expr_ty("after_assign"), ws.ty("Foo|Bar"));
    }

    #[test]
    fn test_assignment_after_pending_return_cast_guard_drops_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Creature

            ---@class Player: Creature

            ---@class Monster: Creature

            ---@param creature Creature
            ---@return boolean
            ---@return_cast creature Player else Monster
            local function is_player(creature)
                return true
            end

            local creature ---@type Creature
            local next_creature ---@type Creature

            if not is_player(creature) then
                return
            end

            before_assign = creature
            creature = next_creature
            after_assign = creature
            "#,
        );

        assert_eq!(ws.expr_ty("before_assign"), ws.ty("Player"));
        assert_eq!(ws.expr_ty("after_assign"), ws.ty("Creature"));
    }

    #[test]
    fn test_assignment_after_binary_call_guard_eq_false_drops_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@generic T
            ---@param inst any
            ---@param type `T`
            ---@return TypeGuard<T>
            local function instance_of(inst, type)
                return true
            end

            local value ---@type string|integer|boolean
            local next_value ---@type string|integer|boolean

            if instance_of(value, 'string') == false then
                return
            end

            before_assign = value
            value = next_value
            after_assign = value
            "#,
        );

        assert_eq!(ws.expr_ty("before_assign"), ws.ty("string"));
        assert_eq!(ws.expr_ty("after_assign"), ws.ty("string|integer|boolean"));
    }

    #[test]
    fn test_assignment_after_mixed_eager_and_pending_guards_drops_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Player
            ---@field kind "player"

            ---@class Monster
            ---@field kind "monster"

            ---@param creature Player|Monster
            ---@return boolean
            ---@return_cast creature Player else Monster
            local function is_player(creature)
                return true
            end

            local creature ---@type Player|Monster
            local next_creature ---@type Player|Monster

            if creature.kind ~= "player" then
                return
            end

            if not is_player(creature) then
                return
            end

            before_assign = creature
            creature = next_creature
            after_assign = creature
            "#,
        );

        assert_eq!(ws.expr_ty("before_assign"), ws.ty("Player"));
        assert_eq!(ws.expr_ty("after_assign"), ws.ty("Player|Monster"));
    }

    #[test]
    fn test_assignment_from_nullable_union_keeps_rhs_members() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local x ---@type string?
            local y ---@type number?

            if x then
                x = y
                after_assign = x
            end
            "#,
        );

        assert_eq!(ws.expr_ty("after_assign"), ws.ty("number?"));
    }

    #[test]
    fn test_assignment_from_partially_overlapping_union_keeps_rhs_members() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local x ---@type string|number
            local y ---@type integer|string

            if x == 1 then
                x = y
                after_assign = x
            end
            "#,
        );

        assert_eq!(ws.expr_ty("after_assign"), ws.ty("integer|string"));
    }

    #[test]
    fn test_partial_table_reassignment_preserves_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Foo
            ---@field kind "foo"
            ---@field a integer

            ---@class Bar
            ---@field kind "bar"
            ---@field b integer

            local x ---@type Foo|Bar

            if x.kind == "foo" then
                x = {}
                x.kind = "foo"
                x.a = 1
                after_assign = x
            end
            "#,
        );

        assert_eq!(ws.expr_ty("after_assign"), ws.ty("Foo"));
    }

    #[test]
    fn test_partial_table_reassignment_with_discriminant_preserves_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Foo
            ---@field kind "foo"
            ---@field a integer

            ---@class Bar
            ---@field kind "bar"
            ---@field b integer

            local x ---@type Foo|Bar

            if x.kind == "foo" then
                x = { kind = "foo" }
                x.a = 1
                after_assign = x
            end
            "#,
        );

        assert_eq!(ws.expr_ty("after_assign"), ws.ty("Foo"));
    }

    #[test]
    fn test_exact_string_reassignment_preserves_literal_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local x ---@type string|number

            if x == 1 then
                x = "a"
                after_assign = x
            end
            "#,
        );

        let after_assign = ws.expr_ty("after_assign");
        assert_eq!(ws.humanize_type(after_assign), r#""a""#);
    }

    #[test]
    fn test_assignment_from_broad_string_drops_literal_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local x ---@type "a"|boolean
            local y ---@type string

            if x == "a" then
                x = y
                after_assign = x
            end
            "#,
        );

        assert_eq!(ws.expr_ty("after_assign"), ws.ty("string"));
    }

    #[test]
    fn test_partial_table_reassignment_with_conflicting_discriminant_drops_branch_narrowing() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Foo
            ---@field kind "foo"
            ---@field a integer

            ---@class Bar
            ---@field kind "bar"
            ---@field b integer

            local x ---@type Foo|Bar

            if x.kind == "foo" then
                x = { kind = "bar" }
                after_assign = x
            end
            "#,
        );

        assert_eq!(ws.expr_ty("after_assign"), ws.ty("Foo|Bar"));
    }
}
