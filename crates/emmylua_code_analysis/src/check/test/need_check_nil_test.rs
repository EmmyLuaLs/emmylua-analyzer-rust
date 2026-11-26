#[cfg(test)]
mod test {
    use emmylua_parser::{LuaAstNode, LuaIndexExpr};
    use lsp_types::NumberOrString;
    use tokio_util::sync::CancellationToken;

    use crate::check::test::{check_source, count_by_code};
    use crate::{DiagnosticCode, FileId, LuaType, VirtualWorkspace};

    // 判空后的命名 cast 可以重新加入 nil, 成员投影不得再次使用旧 guard 将其移除.
    #[test]
    fn test_member_cast_restores_nil_after_guard() {
        for assignment in ["", "self.values = values"] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class Holder
                ---@field values? table
                local M = {{}}
                ---@param values? table
                function M:read(values)
                    {assignment}
                    if not self.values then return end
                    ---@cast self.values +?
                    return self.values[1]
                end
                return M
                "#,
            ));
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil),
                1,
                "{assignment}: {diagnostics:#?}"
            );
        }
    }

    // 泛型字段最近写入的 nil 不能被接收者的声明类型覆盖.
    #[test]
    fn test_generic_member_write_preserves_nil() {
        let diagnostics = check_source(
            r#"
            ---@class Holder<T>
            ---@field value T
            local M = {}
            ---@param obj Holder<table>
            local function read(obj)
                ---@cast obj.value +?
                obj.value = nil
                return obj.value[1]
            end
            return M, read
            "#,
        );
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil),
            1,
            "{diagnostics:#?}"
        );
    }

    // 可选别名参与逻辑表达式时, 非空默认表应消除结果中的 nil.
    #[test]
    fn test_logical_alias_table_fallback_is_non_nil() {
        for (fallback, expected) in [("{}", 0), ("{ name = 'fallback' }", 0), ("nil", 1)] {
            let source = format!(
                r#"
                ---@param message table
                local function read(message)
                    local chat = type(message.chat) == 'table' and message.chat or nil
                    local sender = chat and chat.sender or {fallback}
                    return sender.name
                end
                return read
                "#,
            );
            let diagnostics = check_source(&source);
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil),
                expected,
                "{fallback}: {diagnostics:#?}"
            );
        }
    }

    // 当前方法的字段赋值应覆盖其他方法中的初始化, 同时保留当前方法的 nil 写入.
    #[test]
    fn test_member_assignment_uses_current_method_flow() {
        for (value, expected) in [("state", 0), ("nil", 1)] {
            let diagnostics = check_source(&format!(
                r#"
                ---@class State
                ---@field start fun(self: State)

                ---@class Holder
                ---@field state State
                local M = {{}}

                function M:init()
                    self.state = nil
                end

                ---@param state State
                function M:setup(state)
                    self.state = {value}
                    self.state:start()
                end
                return M
                "#,
            ));
            assert_eq!(
                count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil),
                expected,
                "{value}: {diagnostics:#?}"
            );
        }
    }

    // 显式排除 nil 的行内 cast 应保护后续的成员访问.
    #[test]
    fn test_inline_cast_on_nullable_member_prefix() {
        let diagnostics = check_source(
            r#"
            ---@class Value
            ---@field read fun(self: Value, value: integer): integer

            ---@class Holder
            ---@field member? Value
            local Holder = {}

            function Holder:read()
                ---@diagnostic disable-next-line: undefined-field
                if not self.handle then
                    return
                end
                local value = self.member--[[@cast -?]]:read(1)
                self.member:read(2)
                return value
            end
            return Holder
            "#,
        );
        assert_eq!(
            count_by_code(&diagnostics, DiagnosticCode::NeedCheckNil),
            1,
            "{diagnostics:#?}"
        );
    }

    // 带类型标注的模块字典成员在动态索引时应保留值类型.
    #[test]
    fn test_annotated_member_dynamic_index() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let expected = ws.ty("table<string, { value: integer }>");
        let file_id = ws.def(
            r#"
            local M = {}
            ---@type table<string, { value: integer }>
            M.trees = {}

            ---@param name string
            function M.remove(name)
                if M.trees[name] then
                    return M.trees[name].value
                end
            end
            return M
            "#,
        );
        let model = ws.analysis.semantic_model(file_id);
        let index = model
            .chunk()
            .expect("chunk")
            .descendants::<LuaIndexExpr>()
            .find(|index| index.syntax().text() == "M.trees")
            .expect("dictionary member");
        let member = model
            .resolve_member(&index)
            .and_then(|resolved| resolved.member_id)
            .expect("member declaration");
        assert_eq!(model.type_of_member(&member), Some(expected));
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .expect("diagnostics");
        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.code
                != Some(NumberOrString::String(
                    DiagnosticCode::NeedCheckNil.get_name().to_string()
                ))),
            "{diagnostics:#?}"
        );
    }

    #[track_caller]
    fn assert_nil_checks(ws: &VirtualWorkspace, file_id: FileId, expected: usize) {
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .unwrap();
        let code = Some(NumberOrString::String(
            DiagnosticCode::NeedCheckNil.get_name().to_string(),
        ));
        let actual = diagnostics.iter().filter(|d| d.code == code).count();
        assert_eq!(actual, expected, "{diagnostics:#?}");
    }

    #[track_caller]
    fn assert_nil_checks_at(ws: &VirtualWorkspace, file_id: FileId, expected: &[&str]) {
        let model = ws.analysis.semantic_model(file_id);
        let document = model.document_current().expect("test document");
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .expect("diagnostics");
        let code = Some(NumberOrString::String(
            DiagnosticCode::NeedCheckNil.get_name().to_string(),
        ));
        let mut ranges = diagnostics
            .iter()
            .filter(|d| d.code == code)
            .map(|d| document.to_rowan_range(d.range).expect("diagnostic range"))
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| range.start());
        let actual = ranges
            .into_iter()
            .map(|range| document.get_text_slice(range))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{diagnostics:#?}");
    }

    #[test]
    fn test_pairs_only_yields_stored_values() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@class Entry
            ---@field name string

            ---@param entries table<string, Entry?>
            ---@param key string
            local function read(entries, key)
                for _, entry in pairs(entries) do
                    visited = entry
                    print(entry.name)
                end

                local iterate = pairs
                for _, entry in iterate(entries) do
                    print(entry.name)
                end

                -- A lookup can still miss.
                print(entries[key].name)
            end
            "#,
        );
        assert_nil_checks_at(&ws, file_id, &["entries[key]"]);
        assert_eq!(ws.expr_ty("visited"), ws.ty("Entry"));
    }

    #[test]
    fn test_pairs_keeps_nil_from_custom_iterators() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@return integer?, { name: string }?
            local function step(_, previous)
                if not previous then return 1, nil end
                return nil, nil
            end

            for _, direct in step do
                print(direct.name)
            end

            ---@return fun(t: table, previous: integer?): integer?, { name: string }?
            ---@return table
            ---@return nil
            local function pairs(t)
                return step, t, nil
            end
            for _, shadowed in pairs({}) do
                print(shadowed.name)
            end
            "#,
        );
        assert_nil_checks_at(&ws, file_id, &["direct", "shadowed"]);
    }

    #[test]
    fn test_shadowed_next_and_ipairs_use_their_own_iterators() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@class Entries
            local mt = {}

            ---@return fun(): integer?, { name: string }
            function mt:__pairs() end

            ---@return fun(): integer?, { name: string }
            function mt:__ipairs() end

            ---@return integer?, { name: string }?
            local function step(_, previous)
                if not previous then return 1, nil end
                return nil, nil
            end

            ---@return fun(t: table, previous: integer?): integer?, { name: string }?
            ---@return table
            local function replacement(t)
                return step, t
            end

            local next = replacement
            local ipairs = replacement

            ---@param entries Entries
            local function read(entries)
                for _, by_next in next(entries) do
                    print(by_next.name)
                end
                for _, by_ipairs in ipairs(entries) do
                    print(by_ipairs.name)
                end
            end
            "#,
        );
        assert_nil_checks_at(&ws, file_id, &["by_next", "by_ipairs"]);
    }

    #[test]
    fn test_pairs_keeps_nil_from_metamethod() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            local mt = {}

            ---@return integer?, { name: string }?
            local function step(_, previous)
                if not previous then return 1, nil end
                return nil, nil
            end

            ---@return fun(t: table, previous: integer?): integer?, { name: string }?
            ---@return table
            ---@return nil
            function mt:__pairs()
                return step, self, nil
            end

            for _, entry in pairs(setmetatable({}, mt)) do
                print(entry.name)
            end

            ---@type table<integer, { name: string }?>
            local entries = {}
            for _, inline in pairs(setmetatable(entries, mt)) do
                print(inline.name)
            end

            ---@type table<integer, { name: string }?>
            local assigned = setmetatable({}, mt)
            for _, initialized in pairs(assigned) do
                print(initialized.name)
            end

            ---@type table<integer, { name: string }?>
            local changed = {}
            for _, before_attachment in pairs(changed) do
                print(before_attachment.name)
            end

            setmetatable(changed, mt)
            for _, attached in pairs(changed) do
                print(attached.name)
            end

            ---@param values table<integer, { name: string }?>
            ---@param unknown_mt table
            local function read(values, unknown_mt)
                local decorated = setmetatable(values, mt)
                for _, forwarded in pairs(decorated) do
                    print(forwarded.name)
                end

                for _, unknown in pairs(setmetatable(values, unknown_mt)) do
                    print(unknown.name)
                end
            end
            "#,
        );
        assert_nil_checks_at(
            &ws,
            file_id,
            &[
                "entry",
                "inline",
                "initialized",
                "attached",
                "forwarded",
                "unknown",
            ],
        );
    }

    #[test]
    fn test_pairs_uses_the_metatables_stored_fields() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@class Entry
            ---@field name string

            local mt = setmetatable({}, {
                ---@return (fun(): (fun(): integer, Entry?))
                __index = function()
                    return function()
                        return function() return 1, nil end
                    end
                end,
            })

            ---@type table<integer, Entry>
            local entries = setmetatable({ { name = 'stored' } }, mt)
            for _, entry in pairs(entries) do
                print(entry.name)
            end
            "#,
        );
        assert_nil_checks_at(&ws, file_id, &[]);
    }

    #[test]
    fn test_pairs_metatable_follows_aliases_and_branches() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@return integer?, { name: string }?
            local function step(_, previous)
                if not previous then return 1, nil end
                return nil, nil
            end

            local mt = {}
            ---@return fun(t: table, previous: integer?): integer?, { name: string }?
            ---@return table
            ---@return nil
            function mt:__pairs()
                return step, self, nil
            end

            ---@param flag boolean
            local function read(flag)
                ---@type table<integer, { name: string }>
                local entries = { { name = "stored" } }
                local alias = entries
                local attach = setmetatable
                if flag then
                    assert(attach(alias, mt))
                    for _, attached in pairs(entries) do print(attached.name) end
                else
                    for _, plain in pairs(entries) do print(plain.name) end
                end

                for _, merged in pairs(entries) do print(merged.name) end

                setmetatable(alias, nil)
                for _, cleared in pairs(entries) do print(cleared.name) end

                if attach(alias, mt) then
                    for _, conditioned in pairs(entries) do print(conditioned.name) end
                end
            end
            "#,
        );
        assert_nil_checks_at(&ws, file_id, &["attached", "merged", "conditioned"]);
    }

    #[test]
    fn test_dynamic_index_uses_metatable_value() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            local options = setmetatable({}, {
                __index = function() return { text = 'hello' } end,
            })
            local throwing = setmetatable({}, {
                __index = function(_, key)
                    if key == 1 then return { text = 'hello' } end
                    error('missing key')
                end,
            })
            ---@param index integer
            ---@param key string
            local function read(index, key)
                print(throwing[index].text)
                print(options[index].text)
                print(options[key].text)
                integer_value = options[index]
                string_value = options[key]
            end
            "#,
        ));
        assert!(matches!(
            ws.expr_ty("integer_value"),
            LuaType::TableConst(_)
        ));
        assert!(matches!(ws.expr_ty("string_value"), LuaType::TableConst(_)));
    }

    #[test]
    fn test_dynamic_index_on_recursive_option_accessor() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@param get fun(key: string): any
            local function new_options(get)
                return setmetatable({}, {
                    __index = function(_, key)
                        if type(key) == 'number' then return new_options(get) end
                        return get(key)
                    end,
                })
            end
            ---@param get fun(key: string): any
            ---@param index integer
            local function read(get, index)
                local options = new_options(get)
                print(options[index].text)
            end
            "#,
        ));
    }

    #[test]
    fn test_dynamic_index_keeps_possible_nil() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            local options = setmetatable({}, {
                ---@return {text: string}?
                __index = function() return nil end,
            })
            local plain = { text = { text = 'hello' } }
            ---@param index integer
            ---@param key string
            local function read(index, key)
                print(options[index].text)
                print(options[key].text)
                print(plain[index].text)
                print(plain[key].text)
            end
            "#,
        );
        assert_nil_checks(&ws, file_id, 4);
    }

    #[test]
    fn test_dynamic_index_keeps_inferred_missing_returns() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            local empty = setmetatable({}, { __index = function() end })
            local conditional = setmetatable({}, {
                __index = function(_, key)
                    if key == 1 then return { text = 'hello' } end
                end,
            })
            local bare = setmetatable({}, { __index = function() return end })
            local partial = setmetatable({}, {
                __index = function(_, key)
                    if key == 1 then return end
                    return { text = 'hello' }
                end,
            })
            local raw = setmetatable({}, {
                __index = function(self, key) return rawget(self, key) end,
            })
            ---@param index integer
            ---@param key string
            local function read(index, key)
                print(empty[index].text)
                print(empty[key].text)
                print(conditional[index].text)
                print(conditional[key].text)
                print(bare[index].text)
                print(bare[key].text)
                print(partial[index].text)
                print(partial[key].text)
                print(raw[index].text)
                print(raw[key].text)
            end
            "#,
        );
        assert_nil_checks(&ws, file_id, 10);
    }

    #[test]
    fn test_dynamic_index_keeps_forwarded_missing_returns() {
        for body in [
            "",
            "if flag then return { text = 'hello' } end",
            "local function error() end; error()",
            "error = function() end; error()",
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let source = format!(
                r#"
                ---@param flag boolean
                local function maybe(flag) {body} end
                local forward = maybe
                local options = setmetatable({{}}, {{
                    __index = function(_, key) return forward(key == 1) end,
                }})
                ---@param index integer
                ---@param key string
                local function read(index, key)
                    print(options[index].text)
                    print(options[key].text)
                end
                "#,
            );
            let file_id = ws.def(&source);
            assert_nil_checks(&ws, file_id, 2);
        }
    }

    #[test]
    fn test_dynamic_index_keeps_generic_callback_nil() {
        for (annotation, body, nil_checks) in [
            ("", "if flag then return { text = 'hello' } end", 2),
            (
                "---@return {text: string}?",
                "if flag then return { text = 'hello' } end",
                2,
            ),
            ("", "return { text = 'hello' }", 0),
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let source = format!(
                r#"
                ---@generic T
                ---@param f fun(flag: boolean): T
                ---@param flag boolean
                ---@return T
                local function call(f, flag) return f(flag) end
                ---@param flag boolean
                {annotation}
                local function value(flag) {body} end
                local options = setmetatable({{}}, {{
                    __index = function(_, key) return call(value, key == 1) end,
                }})
                ---@param index integer
                ---@param key string
                local function read(index, key)
                    print(options[index].text)
                    print(options[key].text)
                end
                "#,
            );
            let file_id = ws.def(&source);
            assert_nil_checks(&ws, file_id, nil_checks);
        }
    }

    #[test]
    fn test_dynamic_index_keeps_overloaded_generic_callback_nil() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@generic T
            ---@param f fun(flag: boolean): T
            ---@param flag boolean
            ---@return T
            local function call(f, flag) return f(flag) end
            ---@param flag boolean
            local function maybe(flag)
                if flag then return { text = 'hello' } end
            end
            ---@overload fun(self: table, key: string): nil
            local function lookup(_, key) return call(maybe, key == 1) end
            local options = setmetatable({}, { __index = lookup })
            ---@param index integer
            ---@param key string
            local function read(index, key)
                print(options[index].text)
                print(options[key].text)
            end
            "#,
        );
        assert_nil_checks(&ws, file_id, 2);
    }

    #[test]
    fn test_dynamic_index_keeps_nil_after_later_error_replacement() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            local function maybe(flag)
                if flag then return { text = 'hello' } end
                error('missing')
            end
            error = function() end
            local options = setmetatable({}, {
                __index = function(_, key) return maybe(key == 1) end,
            })
            ---@param index integer
            local function read(index) print(options[index].text) end
            "#,
        ));
    }

    #[test]
    fn test_dynamic_index_keeps_nil_after_large_integer_comparison() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            local options = setmetatable({}, {
                __index = function()
                    if 9007199254740992 == 9007199254740993 then
                        return { text = 'hello' }
                    end
                end,
            })
            ---@param index integer
            local function read(index) print(options[index].text) end
            "#,
        ));
    }

    const CORRELATED_PICK: &str = r#"
            ---@param left boolean
            ---@param right boolean
            ---@return boolean, boolean, string?
            ---@return_overload true, true, string
            ---@return_overload true, false, nil
            ---@return_overload false, true, nil
            local function pick(left, right)
                if left and right then return true, true, 'hello' end
                if left then return true, false, nil end
                return false, true, nil
            end
    "#;

    #[test]
    fn test_correlated_guards_keep_nil_after_discriminant_write() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            &format!(
                r#"
            {CORRELATED_PICK}
            ---@param left boolean
            ---@param right boolean
            ---@param replace boolean
            local function read(left, right, replace)
                local a, b, value = pick(left, right)
                if replace then b = true end
                if a then
                    if b then print(value:upper()) end
                end
            end
            "#,
            ),
        ));
    }

    #[test]
    fn test_correlated_guards_keep_nil_after_captured_write() {
        for (body, call, nil_checks) in [
            ("b = true", "change()", 1),
            ("value = nil", "change()", 1),
            ("print(a)", "change()", 0),
            ("b = b", "change()", 0),
            ("b = true", "", 0),
            ("value = nil", "", 0),
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let file_id = ws.def(&format!(
                r#"
                {CORRELATED_PICK}
                ---@param left boolean
                ---@param right boolean
                local function read(left, right)
                    local a, b, value = pick(left, right)
                    local function change() {body} end
                    if a then
                        {call}
                        if b then print(value:upper()) end
                    end
                end
                "#,
            ));
            assert_nil_checks(&ws, file_id, nil_checks);
        }
    }

    #[test]
    fn test_correlated_guards_use_the_same_return_row() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(&format!(
            r#"
            {CORRELATED_PICK}
            ---@param left boolean
            ---@param right boolean
            local function read(left, right)
                local a, b, value = pick(left, right)
                if a then
                    if b then successful = value end
                    if not b then missing = value end
                end
            end
        "#,
        ));
        assert_eq!(ws.expr_ty("successful"), ws.ty("string"));
        assert_eq!(ws.expr_ty("missing"), ws.ty("nil"));
    }

    #[test]
    fn test_correlated_guards_keep_nil_after_write() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param flag boolean
            ---@return boolean, string?
            ---@return_overload true, string
            ---@return_overload false, nil
            local function pick(flag)
                if flag then return true, 'hello' end
                return false, nil
            end
            ---@param flag boolean
            ---@param replace boolean
            local function read(flag, replace)
                local ok, value = pick(flag)
                if ok then
                    success = value
                    print(value:upper())
                end
                if not ok then
                    failure = value
                    print(value:upper())
                end
                if replace then value = nil end
                if ok then
                    after_write = value
                    print(value:upper())
                end
            end
            "#,
        );
        assert_eq!(ws.expr_ty("success"), ws.ty("string"));
        assert_eq!(ws.expr_ty("failure"), ws.ty("nil"));
        assert_eq!(ws.expr_ty("after_write"), ws.ty("string?"));
        assert_nil_checks(&ws, file_id, 2);
    }

    #[test]
    fn test_guard_after_branch_assignment() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param value string?
            ---@param flag boolean
            local function read(value, flag)
                if flag then value = 'hello' end
                print(value and value:upper())
            end

            ---@param hash (integer|fun(): integer)?
            local function normalize(hash)
                if type(hash) == 'number' then
                    local number = hash
                    hash = function() return number end
                end
                print(hash and hash())
            end
            "#,
        );
        assert_nil_checks(&ws, file_id, 0);
    }

    #[test]
    fn test_type_guards_follow_the_builtin_function() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param aliased string?
            ---@param reassigned string?
            local function check_alias(aliased, reassigned)
                local classify = type
                if classify(aliased) == 'string' then aliased:upper() end

                local kind = classify(aliased)
                if kind == 'string' then aliased:upper() end

                local is_string = classify(aliased) == 'string'
                if is_string then aliased:upper() end

                classify = function(_) return 'string' end
                if classify(reassigned) == 'string' then reassigned:upper() end
            end

            local function type(_) return 'string' end

            ---@param shadowed string?
            local function check_shadowing(shadowed)
                if type(shadowed) == 'string' then shadowed:upper() end

                local kind = type(shadowed)
                if kind == 'string' then shadowed:upper() end

                local is_other = type(shadowed) ~= 'string'
                if is_other then return end
                shadowed:upper()
            end
            "#,
        );

        assert_nil_checks_at(
            &ws,
            file_id,
            &["reassigned", "shadowed", "shadowed", "shadowed"],
        );
    }

    #[test]
    fn test_saved_type_guards_keep_nil_after_writes() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param by_kind string?
            ---@param by_flag string?
            ---@param changed string?
            ---@param unchanged string?
            ---@param replacement string?
            local function read(by_kind, by_flag, changed, unchanged, replacement)
                local classify = type
                local kind = classify(by_kind)
                kind = 'string'
                if kind == 'string' then by_kind:upper() end

                local flag = classify(by_flag) == 'string'
                flag = true
                if flag then by_flag:upper() end

                local function replace() changed = replacement end
                local was_string = classify(changed) == 'string'
                replace()
                if was_string then changed:upper() end

                unchanged = replacement
                local saved_kind = classify(unchanged)
                classify = function(_) return 'number' end
                if saved_kind == 'string' then unchanged:upper() end
                unchanged = replacement
            end
            "#,
        );

        assert_nil_checks_at(&ws, file_id, &["by_kind", "by_flag", "changed"]);
    }

    #[test]
    fn test_saved_type_guard_keeps_correlated_nil_after_write() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param flag boolean
            ---@return string | integer, string?
            ---@return_overload string, string
            ---@return_overload integer, nil
            local function pick(flag)
                if flag then return 'tag', 'text' end
                return 1, nil
            end

            ---@param flag boolean
            local function read(flag)
                local classify = type
                local tag, result = pick(flag)
                local kind = classify(tag)
                kind = 'string'
                if kind == 'string' then result:upper() end
            end
            "#,
        );

        assert_nil_checks_at(&ws, file_id, &["result"]);
    }

    #[test]
    fn test_saved_type_guard_keeps_all_table_return_rows() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        ws.def(
            r#"
            ---@class GuardRecord
            ---@field name string

            ---@param table_row boolean
            ---@param record_row boolean
            ---@return table | GuardRecord | string, string?
            ---@return_overload table, string
            ---@return_overload GuardRecord, nil
            ---@return_overload string, string
            local function pick(table_row, record_row)
                if table_row then return {}, 'text' end
                if record_row then
                    ---@type GuardRecord
                    local tag = { name = 'name' }
                    return tag, nil
                end
                return 'tag', 'text'
            end

            ---@param table_row boolean
            ---@param record_row boolean
            local function read(table_row, record_row)
                local classify = type
                local tag, result = pick(table_row, record_row)
                local kind = classify(tag)
                if kind == 'table' then
                    table_result = result
                else
                    string_result = result
                end
            end
            "#,
        );

        assert_eq!(ws.expr_ty("table_result"), ws.ty("string?"));
        assert_eq!(ws.expr_ty("string_result"), ws.ty("string"));
    }

    #[test]
    fn test_saved_type_guard_tracks_loop_writes() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param refreshed string?
            ---@param stale string?
            ---@param after_loop string?
            ---@param run boolean
            local function read(refreshed, stale, after_loop, run)
                local classify = type
                local stale_kind = classify(stale)
                local before_loop = classify(after_loop)

                while run do
                    local fresh_kind = classify(refreshed)
                    if fresh_kind == 'string' then refreshed:upper() end
                    if stale_kind == 'string' then stale:upper() end
                    refreshed = nil
                    stale = nil
                    after_loop = nil
                end

                if before_loop == 'string' then after_loop:upper() end
            end
            "#,
        );

        assert_nil_checks_at(&ws, file_id, &["stale", "after_loop"]);
    }

    #[test]
    fn test_saved_type_guard_follows_loop_exits() {
        for header in [
            "while run do",
            "for _ = 1, count do",
            "for _ in pairs(entries) do",
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let file_id = ws.def(&format!(
                r#"
                ---@param on_other_branch string?
                ---@param on_return string?
                ---@param on_break string?
                ---@param stop boolean
                ---@param run boolean
                ---@param count integer
                ---@param entries table
                local function read(on_other_branch, on_return, on_break, stop, run, count, entries)
                    local classify = type
                    local branch_kind = classify(on_other_branch)
                    if stop then
                        {header}
                            on_other_branch = nil
                            break
                        end
                        return
                    end
                    if branch_kind == 'string' then on_other_branch:upper() end

                    local return_kind = classify(on_return)
                    {header}
                        on_return = nil
                        return
                    end
                    if return_kind == 'string' then on_return:upper() end

                    local break_kind = classify(on_break)
                    {header}
                        on_break = nil
                        break
                    end
                    if break_kind == 'string' then on_break:upper() end
                end
                "#,
            ));

            assert_nil_checks_at(&ws, file_id, &["on_break"]);
        }
    }

    #[test]
    fn test_saved_type_guard_follows_loop_back_edges() {
        for (header, footer) in [
            ("for _ in pairs(entries) do", "end"),
            ("repeat", "until done"),
        ] {
            let mut ws = VirtualWorkspace::new_with_init_std_lib();
            let file_id = ws.def(&format!(
                r#"
                ---@param stale string?
                ---@param refreshed string?
                ---@param entries table
                ---@param done boolean
                local function read(stale, refreshed, entries, done)
                    local kind = type(stale)
                    {header}
                        if kind == 'string' then stale:upper() end
                        stale = nil
                    {footer}

                    {header}
                        local fresh_kind = type(refreshed)
                        if fresh_kind == 'string' then refreshed:upper() end
                        refreshed = nil
                    {footer}
                end
                "#,
            ));

            assert_nil_checks_at(&ws, file_id, &["stale"]);
        }
    }

    #[test]
    fn test_saved_type_guard_follows_goto_paths() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param forward string?
            ---@param indirect string?
            ---@param skipped string?
            ---@param run boolean
            local function read(forward, indirect, skipped, run)
                local forward_kind = type(forward)
                while run do
                    goto write
                    ::write::
                    forward = nil
                    break
                end
                if forward_kind == 'string' then forward:upper() end

                local indirect_kind = type(indirect)
                while run do
                    goto start
                    ::write::
                    indirect = nil
                    goto leave
                    ::start::
                    goto write
                    ::leave::
                    break
                end
                if indirect_kind == 'string' then indirect:upper() end

                local skipped_kind = type(skipped)
                while run do
                    goto leave
                    ::unused::
                    skipped = nil
                    ::leave::
                    break
                end
                if skipped_kind == 'string' then skipped:upper() end
            end
            "#,
        );

        assert_nil_checks_at(&ws, file_id, &["forward", "indirect"]);
    }

    #[test]
    fn test_saved_type_guard_ignores_writes_in_unused_local_function() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param value string?
            local function read(value)
                local kind = type(value)
                local function unused() value = nil end
                if kind == 'string' then value:upper() end
            end
            "#,
        );

        assert_nil_checks_at(&ws, file_id, &[]);
    }

    #[test]
    fn test_saved_nil_check_narrows_global() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@type string?
            value = nil

            local function read()
                local present = value ~= nil
                if present then value:upper() end
            end
            "#,
        ));
    }

    #[test]
    fn test_saved_type_guard_tracks_global_writes() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param optional string?
            ---@param run boolean
            local function read(optional, run)
                unchanged = optional
                local unchanged_kind = type(unchanged)
                if unchanged_kind == 'string' then unchanged:upper() end

                replaced = optional
                local old_kind = type(replaced)
                replaced = nil
                if old_kind == 'string' then replaced:upper() end

                through_environment = optional
                local classify = type
                local environment_kind = classify(through_environment)
                _G.through_environment = nil
                if environment_kind == 'string' then through_environment:upper() end

                loop_value = optional
                local before_loop = classify(loop_value)
                while run do loop_value = nil; break end
                if before_loop == 'string' then loop_value:upper() end
            end
            "#,
        );

        assert_nil_checks_at(
            &ws,
            file_id,
            &["replaced", "through_environment", "loop_value"],
        );
    }

    #[test]
    fn test_type_guard_comparison_operators() {
        for (operator, when_true, when_false) in [
            ("==", "string", "nil"),
            ("~=", "nil", "string"),
            ("<", "string?", "string?"),
            ("<=", "string?", "string?"),
            (">", "string?", "string?"),
            (">=", "string?", "string?"),
        ] {
            for (setup, left, right) in [
                ("", "type(value)", "'string'"),
                ("local classify = type", "('string')", "(classify(value))"),
                ("local kind = type(value)", "kind", "'string'"),
            ] {
                let mut ws = VirtualWorkspace::new_with_init_std_lib();
                ws.def(&format!(
                    r#"
                    ---@param value string?
                    local function compare(value)
                        {setup}
                        if {left} {operator} {right} then
                            on_true = value
                        else
                            on_false = value
                        end
                    end
                    "#,
                ));

                assert_eq!(
                    (ws.expr_ty("on_true"), ws.expr_ty("on_false")),
                    (ws.ty(when_true), ws.ty(when_false)),
                    "{left} {operator} {right}",
                );
            }
        }
    }

    #[test]
    fn test_guards_do_not_hide_nil_after_merge_or_write() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let file_id = ws.def(
            r#"
            ---@param value string?
            ---@param flag boolean
            local function read(value, flag)
                if flag then value = 'hello' end
                print(value and value:upper())
                print(value:upper())
                if value then
                    value = nil
                    print(value:upper())
                end
            end
            "#,
        );
        assert_nil_checks(&ws, file_id, 2);
    }

    fn check_class_array_guard(element_type: &str, body: &str, expected_codes: &[DiagnosticCode]) {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        let mut emmyrc = ws.get_emmyrc();
        emmyrc.strict.array_index = true;
        ws.update_emmyrc(emmyrc);
        let file_id = ws.def(&format!(
            r#"
            ---@class Container
            ---@field items {element_type}[]
            local _Container = {{}}

            ---@param t Container
            ---@param _other Container
            ---@param _index integer
            local function read(t, _other, _index)
                {body}
            end
            read({{ items = {{ 'hello' }} }}, {{ items = {{}} }}, 1)
            "#,
        ));
        let diagnostics = ws
            .analysis
            .diagnose_file(file_id, CancellationToken::new())
            .expect("diagnostics");
        let expected = expected_codes
            .iter()
            .map(|code| Some(NumberOrString::String(code.get_name().to_string())))
            .collect::<Vec<_>>();
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>(),
            expected,
            "{body}: {diagnostics:#?}"
        );
    }

    #[test]
    fn test_class_array_element_guards() {
        for body in [
            "if t.items[1] then print(t.items[1]:sub(1)) end",
            "if not t.items[1] then return end; print(t.items[1]:sub(1))",
            "if t.items[1] ~= nil then print(t.items[1]:sub(1)) end",
            "if t.items[1] == nil then return end; print(t.items[1]:sub(1))",
            "if t.items[1] then t.items[2] = 'other'; print(t.items[1]:sub(1)) end",
            "t.items = {}; if t.items[1] then print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[1] = t.items[1]:sub(1) end",
            "t.items[1] = t.items[2]; if t.items[1] then print(t.items[1]:sub(1)) end",
        ] {
            check_class_array_guard("string", body, &[]);
        }
    }

    #[test]
    fn test_class_array_element_guard_writes() {
        for body in [
            "if t.items[1] then t.items[1] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[1.0] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[1e0] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items = {}; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t = _other; print(t.items[1]:sub(1)) end",
            "if t.items[1] then t.items[_index] = nil; print(t.items[1]:sub(1)) end",
            "if t.items[1] then if _index == 1 then t.items = {} end; print(t.items[1]:sub(1)) end",
        ] {
            check_class_array_guard(
                "string",
                body,
                &[
                    DiagnosticCode::ParamTypeMismatch,
                    DiagnosticCode::NeedCheckNil,
                ],
            );
        }
    }

    #[test]
    fn test_class_array_element_guard_scope() {
        for body in [
            "print(t.items[1]:sub(1))",
            "if t.items[1] then print(t.items[2]:sub(1)) end",
            "if t.items[1] then end; print(t.items[1]:sub(1))",
            "if t.items[1] then print(_other.items[1]:sub(1)) end",
            "if t.items[1] then local function later() print(t.items[1]:sub(1)) end; t.items = {}; later() end",
            "if (t.items[1] ~= nil) ~= nil then print(t.items[1]:sub(1)) end",
            "if (not not t.items[1]) ~= nil then print(t.items[1]:sub(1)) end",
        ] {
            check_class_array_guard(
                "string",
                body,
                &[
                    DiagnosticCode::ParamTypeMismatch,
                    DiagnosticCode::NeedCheckNil,
                ],
            );
        }
        check_class_array_guard(
            "string",
            "if t.items[1] then local t = _other; print(t.items[1]:sub(1)) end",
            &[
                DiagnosticCode::RedefinedLocal,
                DiagnosticCode::ParamTypeMismatch,
                DiagnosticCode::NeedCheckNil,
            ],
        );
    }

    #[test]
    fn test_class_array_element_guard_preserves_assignment_type() {
        for (value, expected) in [
            ("'hello'", &[] as &[DiagnosticCode]),
            ("1", &[DiagnosticCode::AssignTypeMismatch]),
        ] {
            check_class_array_guard(
                "(string|number)",
                &format!(
                    r#"
                    t.items[1] = {value}
                    if t.items[1] then
                        ---@type string
                        local _s = t.items[1]
                    end
                    "#,
                ),
                expected,
            );
        }
    }

    #[test]
    fn test_issue_245() {
        let mut ws = VirtualWorkspace::new();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        local a --- @type table?
        local _ = (a and a.type == 'change') and a.field
        "#
        ));
    }
    #[test]
    fn test_issue_402() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@class A
            local a = {}

            ---@param self table?
            function a.new(self)
                if self then
                    self.a = 1
                end
            end
        "#
        ));
    }

    #[test]
    fn test_issue_474() {
        let mut ws = VirtualWorkspace::new_with_init_std_lib();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@class Range4
            ---@class TSNode: userdata
            ---@field range fun(self: TSNode): Range4

            ---@param node_or_range TSNode|Range4
            ---@return Range4
            function foo(node_or_range)
                if type(node_or_range) == 'table' then
                    return node_or_range
                else
                    return node_or_range:range()
                end
            end
            "#
        ));
    }

    #[test]
    fn test_cast() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class Cast1
            ---@field get fun(self: self, a: number): Cast1?
            ---@field get2 fun(self: self, a: number): Cast1?
        "#,
        );
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
                ---@type Cast1
                local A

                local a = A:get(1) --[[@cast -?]]
                    :get2(2)
            "#
        ));
    }

    #[test]
    fn test_issue_895_891() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        local t = {
        123,
        234,
        345,
        }

        ---@param id number
        function test(id) end

        for i = 1, #t do
            test(t[i]) -- expected 'number' but found (123|234|345)?
        end
        "#,
        ));
    }

    #[test]
    fn test_issue_886() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::AssignTypeMismatch,
            r#"
        ---@type string[]
        local a = {}

        -- if #a == 0 then return end
        if not a[1] then return end

        -- ---@type string
        -- local s = a[1]

        ---@type string
        local s = a[#a]
        "#,
        ));
    }

    #[test]
    fn test_asserted_index_assignment_prefix_is_not_nil() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type integer[][]
        local res
        res[1][1] = assert(res[1])[2]
        "#,
        ));
    }

    #[test]
    fn test_numeric_for_len_expr_narrows_loop_body_value_to_non_nil() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type false|fun(...)[]?
        local calls

        -- `#calls` itself may raise; the body read is what this pins.
        ---@diagnostic disable-next-line: need-check-nil
        for i = 1, #calls do
            calls[i](...)
        end
        "#,
        ));
    }

    // `not` 守卫的 else 分支同样应去掉 nil, 不再要求判空.
    #[test]
    fn test_not_member_else_branch_narrows() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@class Fix01
        ---@field _main Fix01.T?
        local M = {}

        ---@class Fix01.T
        ---@field get fun(): boolean

        function M:bind()
            if not self._main then
                return true
            else
                local child = self._main:get()
                return child
            end
        end
        return M
        "#,
        ));
    }

    // `not` 的真分支把成员收窄为 nil, 其中的成员访问仍应判空.
    #[test]
    fn test_not_member_true_branch_narrows_to_nil() {
        let mut ws = VirtualWorkspace::new();
        let file = ws.def(
            r#"
        ---@class Fix01
        ---@field _main Fix01.T?
        local M = {}

        ---@class Fix01.T
        ---@field get fun(): boolean

        function M:bind()
            if not self._main then
                local child = self._main:get()
                return child
            end
        end
        return M
        "#,
        );
        let model = ws.analysis.semantic_model(file);
        let chunk = model.chunk().expect("chunk");
        let access = chunk
            .descendants::<LuaIndexExpr>()
            .filter(|index| index.syntax().text() == "self._main")
            .last()
            .expect("member access");
        assert_eq!(
            model.type_of_expr_at(access.get_syntax_id(), access.get_position()),
            LuaType::Nil
        );
    }

    // 运行时成员写入之后, `and` / `if` 守卫仍应把同一访问路径收窄为非空.
    #[test]
    fn test_runtime_member_write_then_guard_narrows() {
        for body in [
            "local aid = runtime.client and runtime.client.aid",
            "if runtime.client then local aid = runtime.client.aid end",
        ] {
            let mut ws = VirtualWorkspace::new();
            let source = format!(
                r#"
        ---@class Fix01.Client
        ---@field aid integer
        ---@class Fix01.Runtime
        ---@field client? Fix01.Client
        ---@type Fix01.Runtime
        local runtime = {{}}
        local M = {{}}
        runtime.client = runtime.client or nil
        function M.next_request_id()
            {body}
        end
        return M
        "#
            );
            assert!(
                ws.has_no_diagnostic(DiagnosticCode::NeedCheckNil, &source),
                "{body}"
            );
        }
    }

    #[test]
    fn test_not_member_early_return_guard() {
        let mut ws = VirtualWorkspace::new();
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@class Fix01
        ---@field _main Fix01.T?
        local M = {}

        ---@class Fix01.T
        ---@field get fun(): boolean

        function M:bind()
            if not self._main then
                return
            end
            local child = self._main:get()
            return child
        end
        return M
        "#,
        ));
    }

    #[test]
    fn test_different_asserted_index_assignment_prefix_still_needs_nil_check() {
        let mut ws = VirtualWorkspace::new();
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
        ---@type integer[][]
        local res
        res[1][1] = assert(res[2])[2]
        "#,
        ));
    }

    /// Arithmetic on nil raises, as indexing nil does. Each operand that may be nil is reported once.
    #[test]
    fn test_arithmetic_on_nullable_operand() {
        for (body, expected) in [
            ("return x - 1", 1),
            ("return 1 + x", 1),
            ("return x * x", 2),
            ("return x / 2, x // 2, x % 2, x ^ 2", 4),
            ("return -x", 1),
            // `#y` raises on nil, but a length is never nil: only `y` is reported.
            ("return #y + 1", 1),
            // A missing member is undefined-field's to report, its length too.
            ("return #b.missing + 1", 0),
            // Guarded, defaulted, or not nil at all.
            ("return x and x - 1", 0),
            ("if x then return x - 1 end", 0),
            ("return (x or 0) - 1", 0),
            ("return a + 1, -a", 0),
            ("local s = 0 for i = 1, #t do s = s + t[i] end return s", 0),
            // Neither concatenation nor bitwise operators are arithmetic here (0.25.1 did not check them).
            ("return x .. '', x & 1", 0),
            // An operand typed bare `nil` is an inference gap (a field of a `table` call result),
            // not a value that may be nil: left alone.
            ("return tb().x + 1", 0),
        ] {
            let source = format!(
                r#"
                ---@alias NcnInfo table

                ---@return NcnInfo
                local function tb() return {{}} end

                ---@class NcnBox
                local NcnBox = {{}}

                ---@param x integer?
                ---@param a any
                ---@param t integer[]
                ---@param y any?
                ---@param b NcnBox
                local function f(x, a, t, y, b)
                    {body}
                end
                return f
                "#
            );
            assert_eq!(
                count_by_code(&check_source(&source), DiagnosticCode::NeedCheckNil),
                expected,
                "{body}"
            );
        }
    }

    #[test]
    fn test_length_of_nullable_operand() {
        let mut ws = VirtualWorkspace::new();
        // `#nil` raises, as `nil.x` does.
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@param w string?
            local function f(w) return #w end
            return f
        "#
        ));
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            ---@param w string?
            local function f(w) return w and #w end
            return f
        "#
        ));
        // Where it yields a value, the length is an integer, not the operand's `string?`.
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::ParamTypeMismatch,
            r#"
            ---@param n integer
            local function take(n) end
            ---@param w string?
            local function f(w) take(#w) take(1 + #w - 1) end
            return f
        "#
        ));
    }

    fn nil_messages(source: &str) -> Vec<String> {
        check_source(source)
            .into_iter()
            .filter(|d| d.code == DiagnosticCode::NeedCheckNil)
            .map(|d| d.message)
            .collect()
    }

    /// A length is never nil, so arithmetic on it adds nothing: `#w - 1` reports `w`, not `#w`.
    #[test]
    fn test_arithmetic_on_length_reports_the_operand_once() {
        let source = r#"
            ---@param w string?
            local function f(w) return #w - 1 end
            return f
        "#;
        assert_eq!(nil_messages(source), vec!["w may be nil".to_string()]);
    }

    /// A call that may return nothing is a nilable arithmetic operand: its first result slot
    /// decides, as in the VM's operands. Read raw, `(integer?,integer?,string?...)` hid its nil
    /// (`s:find(..) + 1`), while the parenthesized `(s:find(..)) + 1` was reported.
    #[test]
    fn test_call_that_may_return_nothing_may_be_nil_as_an_operand() {
        let mut ws = VirtualWorkspace::new();
        let maybe = r#"
            local function g(x)
              if x then
                return 1
              end
            end
        "#;
        for use_ in ["print(g(1) + 1)", "print(-g(1))"] {
            assert!(
                !ws.has_no_diagnostic(DiagnosticCode::NeedCheckNil, &format!("{maybe}\n{use_}")),
                "{use_}"
            );
        }
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            local function h()
              return 1
            end
            print(h() + 1, -h())
            "#
        ));
        // A name keeps its flow type: a local from a result list, guarded, is not nil again
        // (blink.cmp `docs.lua`, reduced; projecting names reported both operands here).
        assert!(ws.has_no_diagnostic(
            DiagnosticCode::NeedCheckNil,
            r#"
            --- @param lines string[]
            --- @return number?, number?
            function docs.get_code_block_range(lines, row)
            end
            function docs.extract_detail_from_doc(detail, documentation)
              local doc_lines = docs.split_lines(documentation)
              local code_block_start, code_block_end = docs.get_code_block_range(doc_lines, detail_line)
              if code_block_start ~= nil and code_block_end ~= nil then
                detail_lines = vim.list_slice(doc_lines, code_block_start + 1, code_block_end - 1)
              end
            end
            function docs.split_lines(text)
            end
            "#
        ));
    }
}
