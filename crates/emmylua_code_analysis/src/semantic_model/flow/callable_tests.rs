//! Lua behavior and performance checks for callable flow.

use crate::VirtualWorkspace;

#[test]
fn test_global_signatures_include_main_and_overloads_from_each_file() {
    let mut ws = VirtualWorkspace::new();
    ws.def_file(
        "other.lua",
        r#"
        ---@param value boolean
        ---@return boolean
        ---@overload fun(value: table): table
        function accept(value) return value end
    "#,
    );
    let (mut types, _) = ws.callable_types_with_work(
        r#"
        ---@param value number
        ---@return number
        ---@overload fun(value: string): string
        function accept(value) return value end
        accept(42)
    "#,
        "accept",
    );
    assert_eq!(types.len(), 1);
    types[0].sort();
    assert_eq!(
        types[0],
        vec![
            "fun(value: boolean) -> boolean",
            "fun(value: number) -> number",
            "fun(value: string) -> string",
            "fun(value: table) -> table",
        ]
    );
}

#[test]
fn test_overloaded_functions_preserve_callback_generics() {
    let mut ws = VirtualWorkspace::new();
    ws.def(
        r#"
        ---@generic T
        ---@param callback fun(value: T): T
        ---@param value T
        ---@return T
        ---@overload fun(): nil
        function apply(callback, value)
            return callback(value)
        end
        apply = (apply)

        ---@param value number
        ---@return number
        local function double(value) return value * 2 end
        ---@param value string
        ---@return string
        local function echo(value) return value end

        number_result = apply(double, 42)
        string_result = apply(echo, 'text')
    "#,
    );
    assert_eq!(ws.expr_ty("number_result"), ws.ty("number"));
    assert_eq!(ws.expr_ty("string_result"), ws.ty("string"));
}

#[test]
fn test_callable_types_follow_casts_at_each_read() {
    let mut ws = VirtualWorkspace::new();
    let (types, _) = ws.callable_types_with_work(
        r#"
        ---@param value number
        ---@return number
        ---@overload fun(value: string): string
        ---@overload fun(value: boolean): boolean
        local function accept(value) return value end
        accept(42)
        ---@cast accept -fun(value: string): string
        accept(42)
        ---@cast accept fun(value: table): table
        accept({})
    "#,
        "accept",
    );
    assert_eq!(
        types,
        vec![
            vec![
                "fun(value: number) -> number",
                "fun(value: string) -> string",
                "fun(value: boolean) -> boolean"
            ],
            vec![
                "fun(value: number) -> number",
                "fun(value: boolean) -> boolean"
            ],
            vec!["fun(value: table) -> table"],
        ]
    );
}

#[test]
#[ntest::timeout(5000)]
fn test_repeated_overloaded_calls_do_not_rescan_unrelated_cast_history() {
    for count in [64, 256] {
        for cast_before_calls in [false, true] {
            let calls = "accept(42)\naccept('text')\n".repeat(count);
            let cast = "---@cast other string";
            let source = format!(
                r#"
                ---@param value number
                ---@return number
                ---@overload fun(value: string): string
                local function accept(value) return value end
                ---@type number|string
                local other
                {before}
                {calls}
                {after}
                accept(false)
                return other
            "#,
                before = if cast_before_calls { cast } else { "" },
                after = if cast_before_calls { "" } else { cast }
            );
            let mut ws = VirtualWorkspace::new();
            let (types, work) = ws.callable_types_with_work(&source, "accept");
            let expected = vec![
                "fun(value: number) -> number",
                "fun(value: string) -> string",
            ];
            assert_eq!(types, vec![expected; 2 * count + 1]);
            assert!(
                work > 0 && work < 40 * count as u64,
                "{count} call pairs took {work} flow steps"
            );
        }
    }
}

#[test]
#[ntest::timeout(5000)]
fn test_constant_conditions_do_not_rescan_callable_writes() {
    for count in [16, 64, 600] {
        let source = format!(
            r#"
            ---@return true
            local function allowed() return true end
            ---@type number|string
            local other
            ---@cast other string
            local function later()
                {writes}
            end
            ---@param value number
            ---@return number
            ---@overload fun(value: string): string
            local function accept(value) return value end
            {conditions}
            accept(42)
        "#,
            writes = "allowed = (allowed)\n".repeat(count),
            conditions = "if allowed() then local other = 1 end\n".repeat(count)
        );
        let mut ws = VirtualWorkspace::new();
        let (types, work) = ws.callable_types_with_work(&source, "accept");
        assert_eq!(
            types,
            vec![vec![
                "fun(value: number) -> number",
                "fun(value: string) -> string"
            ]]
        );
        assert!(
            work > 0 && work < 64 * count as u64,
            "{count} conditions took {work} flow steps"
        );
    }
}

#[test]
#[ntest::timeout(5000)]
fn test_deep_branch_casts_merge_surviving_overloads() {
    for count in [4, 64] {
        let branches = r#"
            if choose then
                ---@cast accept -fun(value: string): string
                local true_value = accept
            else
                ---@cast accept -fun(value: string): string
                local false_string = accept
                ---@cast accept -fun(value: boolean): boolean
                local false_value = accept
            end
        "#
        .repeat(count);
        let source = format!(
            r#"
            ---@param value number
            ---@return number
            ---@overload fun(value: string): string
            ---@overload fun(value: boolean): boolean
            local function accept(value) return value end
            local function run(choose)
                {branches}
                accept(42)
            end
        "#
        );
        let mut ws = VirtualWorkspace::new();
        let (types, work) = ws.callable_types_with_work(&source, "accept");
        // Both arms remove string; boolean survives only the true arm.
        assert_eq!(
            types,
            vec![vec![
                "fun(value: number) -> number",
                "fun(value: boolean) -> boolean"
            ]]
        );
        assert!(
            work > 0 && work < 64 * count as u64,
            "{count} branches took {work} flow steps"
        );
    }
}
