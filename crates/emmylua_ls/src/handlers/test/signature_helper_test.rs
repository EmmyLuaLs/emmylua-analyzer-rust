#[cfg(test)]
mod tests {
    use crate::handlers::signature_helper::signature_help;
    use crate::handlers::test_lib::{ProviderVirtualWorkspace, VirtualSignatureHelp, check};
    use googletest::prelude::*;
    use lsp_types::{SignatureHelpContext, SignatureHelpTriggerKind};

    // M3 batch 3 retirement: overload matching / std pcall signature rendering depends on the old signature index and humanize; see migration notes §M3.
    #[gtest]
    fn test_1() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new();
        check!(ws.check_signature_helper(
            r#"
                ---@class Action
                ---@field id fun(self:Action, itemId:integer, ...:integer?):boolean
                ---@overload fun():Action
                Action = {}

                Action:id(1, <??>)
            "#,
            VirtualSignatureHelp {
                target_label: "Action:id(itemId: integer, ...: integer?): boolean".to_string(),
                active_signature: 0,
                active_parameter: 1,
            },
        ));
        Ok(())
    }

    // M3 batch 3 retirement: overload matching / std pcall signature rendering depends on the old signature index and humanize; see migration notes §M3.
    #[gtest]
    fn test_2() -> Result<()> {
        let mut ws = ProviderVirtualWorkspace::new_with_init_std_lib();
        check!(ws.check_signature_helper(
            r#"
                ---@param path string
                local function readFile(path)
                end

                pcall(readFile, <??>)
            "#,
            VirtualSignatureHelp {
                target_label:
                    "pcall(f: sync fun(path: string), path: string): (true|false)".to_string(),
                active_signature: 0,
                active_parameter: 1,
            },
        ));
        Ok(())
    }

    fn signature_labels(source: &str) -> Vec<String> {
        let (source, position) =
            ProviderVirtualWorkspace::handle_file_content(source).expect("signature-help marker");
        let mut ws = ProviderVirtualWorkspace::new();
        let file = ws.def(&source);
        signature_help(
            &ws.analysis,
            file,
            position,
            SignatureHelpContext {
                trigger_kind: SignatureHelpTriggerKind::INVOKED,
                trigger_character: None,
                is_retrigger: false,
                active_signature_help: None,
            },
        )
        .expect("signature help")
        .signatures
        .into_iter()
        .map(|signature| signature.label)
        .collect()
    }

    #[gtest]
    fn test_overload_signatures_follow_reaching_assignments() -> Result<()> {
        for (body, keep_overload) in [
            ("accept = accept\naccept(<??>)", true),
            ("accept = (accept)\naccept(<??>)", true),
            (
                r#"
                local function run(choose)
                    while choose do accept = (accept) end
                    accept(<??>)
                end
                "#,
                true,
            ),
            (
                r#"
                local function run(choose)
                    repeat accept = accept until choose
                    accept(<??>)
                end
                "#,
                true,
            ),
            (
                r#"
                local function run(choose)
                    while choose do accept = (accept) end
                    ---@cast accept -fun(value: string): string
                    accept(<??>)
                end
                "#,
                false,
            ),
            ("---@cast accept -nil\naccept(<??>)", true),
            (
                r#"
                accept(<??>)
                ---@cast accept -fun(value: string): string
                accept(42)
                ---@cast accept fun(value: table): table
                accept({})
                "#,
                true,
            ),
            (
                r#"
                accept(42)
                ---@cast accept -fun(value: string): string
                accept(<??>)
                ---@cast accept fun(value: table): table
                accept({})
                "#,
                false,
            ),
            (
                "local function later()\naccept = replacement\nend\naccept(<??>)",
                true,
            ),
            (
                "local function run(choose)\nif choose then\naccept = replacement\nelse\naccept(<??>)\nend\nend",
                true,
            ),
            ("accept = replacement\naccept(<??>)", false),
            ("if false then accept = replacement end\naccept(<??>)", true),
            (
                "if not false then accept = replacement end\naccept(<??>)",
                false,
            ),
            (
                "if true then accept = replacement else accept = accept end\naccept(<??>)",
                false,
            ),
            (
                "if false then accept = replacement else stop() end\naccept(<??>)",
                false,
            ),
            (
                "---@return never\nfunction global_stop() end\nglobal_stop()\naccept(<??>)",
                true,
            ),
            (
                "---@cast accept fun(value: number): number\naccept(<??>)",
                false,
            ),
            (
                "---@cast accept -fun(value: string): string\naccept(<??>)",
                false,
            ),
            ("accept = replacement\naccept = accept\naccept(<??>)", false),
            ("if true then accept = replacement end\naccept(<??>)", false),
            (
                "local function run(choose)\nif choose then accept = replacement else stop() end\naccept(<??>)\nend",
                false,
            ),
            (
                "---@type number|string\nlocal other\n---@cast other string\n---@type fun()\n---@return never\nlocal maybe_stop = function() error('stop') end\nmaybe_stop = function() end\nmaybe_stop()\naccept(<??>)",
                true,
            ),
            (
                "---@type number|string\nlocal other\n---@cast other string\n---@param value number\n---@return never\n---@overload fun(value: string): string\nlocal function maybe_stop(value) if type(value) == 'string' then return value end error('stop') end\nmaybe_stop('text')\naccept(<??>)",
                true,
            ),
            (
                "---@type fun(): boolean\n---@return false\nlocal choose = function() return false end\nchoose = function() return true end\nif choose() then accept(<??>) else accept = replacement end",
                true,
            ),
            (
                "---@return true\nlocal function always() return true end\nif always() then accept = replacement end\naccept(<??>)",
                false,
            ),
            (
                "---@return false\nlocal function always_false() return false end\nif not always_false() then accept = replacement end\naccept(<??>)",
                false,
            ),
            (
                r#"
                ---@return true
                local function allowed() return true end
                ---@cast allowed fun(): false
                if allowed() then accept = replacement end
                accept(<??>)
                "#,
                true,
            ),
            (
                r#"
                ---@return false
                local function allowed() return false end
                ---@cast allowed fun(): true
                if allowed() then accept = replacement end
                accept(<??>)
                "#,
                false,
            ),
            (
                r#"
                ---@return true
                local function allowed() return true end
                if allowed --[[@as fun(): false]] () then accept = replacement end
                accept(<??>)
                "#,
                true,
            ),
            (
                r#"
                ---@cast stop fun()
                local function run(choose)
                    if choose then accept = replacement else stop() end
                    accept(<??>)
                end
                "#,
                true,
            ),
            (
                r#"
                stop = stop --[[@as fun()]]
                local function run(choose)
                    if choose then accept = replacement else stop() end
                    accept(<??>)
                end
                "#,
                true,
            ),
            (
                r#"
                accept = accept --[[@as fun(value: number): number]]
                accept(<??>)
                "#,
                false,
            ),
            (
                r#"
                accept = (accept) --[[@as fun(value: number): number]]
                accept(<??>)
                "#,
                false,
            ),
            (
                r#"
                local other
                accept, other = accept, 1 --[[@as number]]
                accept(<??>)
                "#,
                true,
            ),
            (
                "---@return false\n---@overload fun(value: string): true\nlocal function choose(value) return type(value) == 'string' end\nif choose('text') then accept(<??>) else accept = replacement end",
                true,
            ),
        ] {
            let source = format!(
                r#"
                ---@param value number
                ---@return number
                ---@overload fun(value: string): string
                local function accept(value) return value end
                ---@param value number
                ---@return number
                local function replacement(value) return value end
                ---@return never
                local function stop() error('stop') end
                {body}
                "#
            );
            let mut expected = vec!["accept(value: number): number"];
            if keep_overload {
                expected.push("accept(value: string): string");
            }
            assert_eq!(signature_labels(&source), expected, "{source}");
        }
        Ok(())
    }

    #[gtest]
    fn test_generic_return_conditions_keep_available_overloads() -> Result<()> {
        for (return_type, argument) in [("T", "false"), ("T", "nil"), ("T|number", "false")] {
            let source = format!(
                r#"
                ---@param value number
                ---@return number
                ---@overload fun(value: string): string
                local function accept(value) return value end
                ---@param value number
                ---@return number
                local function replacement(value) return value end
                ---@generic T
                ---@param value T
                ---@return {return_type}
                local function identity(value) return value end
                if identity({argument}) then accept = replacement end
                accept(<??>)
                "#
            );
            assert_eq!(
                signature_labels(&source),
                vec![
                    "accept(value: number): number",
                    "accept(value: string): string",
                ],
                "{source}"
            );
        }
        Ok(())
    }

    #[gtest]
    fn test_signatures_follow_addition_and_replacement_casts() -> Result<()> {
        for (cast, expected) in [
            (
                "+fun(value: table): table",
                vec![
                    "accept(value: number): number",
                    "accept(value: table): table",
                    "accept(value: string): string",
                ],
            ),
            (
                "fun(value: table): table",
                vec!["accept(value: table): table"],
            ),
        ] {
            let source = format!(
                r#"
                ---@param value number
                ---@return number
                ---@overload fun(value: string): string
                local function accept(value) return value end
                ---@cast accept {cast}
                accept(<??>)
            "#
            );
            assert_eq!(signature_labels(&source), expected, "{source}");
        }
        Ok(())
    }

    #[gtest]
    fn test_reaching_main_signature_survives_a_returning_stop() -> Result<()> {
        for (write, expected) in [
            ("", vec!["accept(value: boolean): boolean"]),
            (
                "stop = function() end",
                vec![
                    "accept(value: boolean): boolean",
                    "accept(value: string): string",
                    "accept(value: number): number",
                ],
            ),
        ] {
            let source = format!(
                r#"
                ---@param value number
                ---@return number
                ---@overload fun(value: string): string
                local function accept(value) return value end
                ---@param value boolean
                ---@return boolean
                local function replacement(value) return value end
                ---@return never
                local function stop() error('stop') end
                {write}
                local function run(choose)
                    if choose then accept = replacement else stop() end
                    accept(<??>)
                end
            "#
            );
            assert_eq!(signature_labels(&source), expected, "{source}");
        }
        Ok(())
    }

    #[gtest]
    fn test_overloaded_signatures_preserve_callback_generics() -> Result<()> {
        assert_eq!(
            signature_labels(
                r#"
                ---@generic T
                ---@param callback fun(value: T): T
                ---@param value T
                ---@return T
                ---@overload fun(): nil
                local function accept(callback, value) return callback(value) end
                accept(<??>)
                "#,
            ),
            vec![
                "accept(callback: fun(value: T) -> T, value: T): T",
                "accept()",
            ]
        );
        Ok(())
    }
}
