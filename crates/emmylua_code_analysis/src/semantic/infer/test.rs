#[cfg(test)]
mod test {
    use emmylua_parser::{
        LuaAstNode, LuaBinaryExpr, LuaCallExpr, LuaExpr, LuaIndexExpr, LuaTableExpr,
    };

    use crate::{
        DiagnosticCode, FileId, NoFlowCacheEntry, VirtualWorkspace,
        semantic::{
            infer::{InferFailReason, infer_expr},
            type_check::check_type_compact,
        },
    };

    fn infer_expr_no_flow(
        db: &crate::DbIndex,
        cache: &mut crate::LuaInferCache,
        expr: LuaExpr,
    ) -> Result<crate::LuaType, InferFailReason> {
        cache.with_no_flow(|cache| infer_expr(db, cache, expr))
    }

    fn infer_expr_type(
        ws: &VirtualWorkspace,
        file_id: FileId,
        expr: LuaExpr,
        no_flow: bool,
    ) -> Result<crate::LuaType, InferFailReason> {
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        let db = semantic_model.get_db();
        let mut cache = semantic_model.get_cache().borrow_mut();

        if no_flow {
            infer_expr_no_flow(db, &mut cache, expr)
        } else {
            infer_expr(db, &mut cache, expr)
        }
    }

    fn assert_infer_expr_type_matches(
        ws: &VirtualWorkspace,
        file_id: FileId,
        expr: LuaExpr,
        expected: &crate::LuaType,
        no_flow: bool,
    ) {
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        let db = semantic_model.get_db();
        let result = infer_expr_type(ws, file_id, expr, no_flow).unwrap();
        assert!(check_type_compact(db, &result, expected).is_ok());
    }

    fn setup_issue_416_workspace() -> (VirtualWorkspace, FileId, LuaCallExpr) {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def_file(
            "test.lua",
            r#"
            ---@class CustomEvent
            ---@field private custom_event_manager? EventManager
            local M = {}

            ---@return EventManager
            function newEventManager()
            end

            function M:event_on()
                if not self.custom_event_manager then
                    self.custom_event_manager = newEventManager()
                end
                local trigger = self.custom_event_manager:get_trigger()
                return trigger
            end
            "#,
        );
        ws.def_file(
            "test2.lua",
            r#"
            ---@class Trigger

            ---@class EventManager
            local EventManager

            ---@return Trigger
            function EventManager:get_trigger()
            end
            "#,
        );

        let tree = ws
            .analysis
            .compilation
            .get_db()
            .get_vfs()
            .get_syntax_tree(&file_id)
            .unwrap();
        let call_expr = tree
            .get_chunk_node()
            .descendants::<LuaCallExpr>()
            .find(|call_expr: &LuaCallExpr| {
                call_expr
                    .syntax()
                    .text()
                    .to_string()
                    .contains("get_trigger")
            })
            .unwrap();

        (ws, file_id, call_expr)
    }

    #[test]
    fn test_infer_expr_no_flow_uses_bound_type_cache_for_unsupported_expr() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            local value = {} --[[@as integer]]
            "#,
        );
        let expected = ws.ty("integer");
        let table_expr = ws.get_node::<LuaTableExpr>(file_id);
        let result = infer_expr_type(&ws, file_id, LuaExpr::TableExpr(table_expr), true);
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_custom_binary() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class AA
        ---@operator pow(number): AA

        ---@type AA
        a = {}
        "#,
        );

        let ty = ws.expr_ty(
            r#"
        a ^ 1
        "#,
        );
        let expected = ws.ty("AA");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_559() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class Origin
            ---@operator add(Origin):Origin

            ---@alias AliasType Origin

            ---@type AliasType
            local x1
            ---@type AliasType
            local x2

            A = x1 + x2
        "#,
        );

        let ty = ws.expr_ty("A");
        let expected = ws.ty("Origin");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_issue_867() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            local a --- @type { foo? : { bar: { baz: number } } }

            local b = a.foo.bar -- a.foo may be nil (correct)

            c = b.baz -- b may be nil (incorrect)
        "#,
        );

        let ty = ws.expr_ty("c");
        let expected = ws.ty("number");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_intersection_call_infers_return_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@type { field: string } & fun(): string
            F = nil
        "#,
        );

        assert_eq!(ws.expr_ty("F()"), ws.ty("string"));
    }

    #[test]
    fn test_infer_expr_list_types_tolerates_infer_failures() {
        let mut ws = VirtualWorkspace::new();
        let code = r#"
            local t ---@type { a: number }

            ---@type string, string
            local y, x

            x, y = t.b, 1
        "#;

        assert!(!ws.check_code_for(DiagnosticCode::UndefinedField, code));
        assert!(!ws.check_code_for(DiagnosticCode::AssignTypeMismatch, code));
    }

    #[test]
    fn test_flow_assign_preserves_doc_type_on_infer_error() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            local t ---@type { a: number }
            local x ---@type string
            x = t.b
            R = x
        "#,
        );

        assert_eq!(ws.expr_ty("R"), ws.ty("nil"));
    }

    #[test]
    fn test_infer_expr_no_flow_caches_binary_expr_result() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def("local t = 1 + 2\n");
        let binary_expr = ws.get_node::<LuaBinaryExpr>(file_id);
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        let db = semantic_model.get_db();
        let mut cache = semantic_model.get_cache().borrow_mut();

        let result = infer_expr_no_flow(db, &mut cache, binary_expr.clone().into()).unwrap();
        assert_eq!(result, crate::LuaType::IntegerConst(3));
        assert!(matches!(
            cache.expr_no_flow_cache.get(&binary_expr.get_syntax_id()),
            Some(NoFlowCacheEntry::Cache(_))
        ));
    }

    #[test]
    fn test_infer_expr_no_flow_declines_index_with_unsupported_prefix() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def("local t = ({}).x\n");
        let index_expr = ws.get_node::<LuaIndexExpr>(file_id);
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        let db = semantic_model.get_db();
        let mut cache = semantic_model.get_cache().borrow_mut();

        let result = infer_expr_no_flow(db, &mut cache, LuaExpr::IndexExpr(index_expr.clone()));
        assert!(matches!(result, Err(InferFailReason::None)));
        assert!(matches!(
            cache.expr_no_flow_cache.get(&index_expr.get_syntax_id()),
            Some(NoFlowCacheEntry::Declined)
        ));
    }

    #[test]
    fn test_infer_expr_no_flow_declines_require_with_unsupported_path() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def("local m = require(1 + 2)\n");
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let semantic_model = ws.analysis.compilation.get_semantic_model(file_id).unwrap();
        let db = semantic_model.get_db();
        let mut cache = semantic_model.get_cache().borrow_mut();

        let result = infer_expr_no_flow(db, &mut cache, LuaExpr::CallExpr(call_expr.clone()));
        assert!(matches!(result, Err(InferFailReason::None)));
        assert!(matches!(
            cache.expr_no_flow_cache.get(&call_expr.get_syntax_id()),
            Some(NoFlowCacheEntry::Declined)
        ));
    }

    #[test]
    fn test_no_flow_issue_416_method_call_returns_method_type() {
        let (ws, file_id, call_expr) = setup_issue_416_workspace();
        let result = infer_expr_type(&ws, file_id, LuaExpr::CallExpr(call_expr), true);
        assert_eq!(ws.humanize_type(result.unwrap()), "Trigger");
    }

    #[test]
    fn test_no_flow_generic_call_supports_binary_arg_expr() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@generic T
            ---@param x T
            ---@return T
            local function id(x) end

            local value = id(1 + 2)
            "#,
        );
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let expected = ws.ty("integer");
        assert_infer_expr_type_matches(&ws, file_id, LuaExpr::CallExpr(call_expr), &expected, true);
    }

    #[test]
    fn test_no_flow_overload_resolution_supports_binary_arg_expr() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@overload fun(x: string): string
            ---@param x integer
            ---@return integer
            local function pick(x) end

            local value = pick(1 + 2)
            "#,
        );
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let expected = ws.ty("integer");
        assert_infer_expr_type_matches(&ws, file_id, LuaExpr::CallExpr(call_expr), &expected, true);
    }

    #[test]
    fn test_no_flow_overload_resolution_keeps_mixed_return_when_known_args_pick_row() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@overload fun(kind: "a", opts: table): integer
            ---@param kind "b"
            ---@param opts table
            ---@return boolean
            local function pick(kind, opts) end

            local value = pick("a", {})
            "#,
        );
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let expected = ws.ty("integer");
        assert_infer_expr_type_matches(&ws, file_id, LuaExpr::CallExpr(call_expr), &expected, true);
    }

    #[test]
    fn test_no_flow_overload_resolution_declines_ambiguous_mixed_returns_with_unknown_arg() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@overload fun(kind: "a", opts: integer): integer
            ---@param kind "a"
            ---@param opts string
            ---@return boolean
            local function pick(kind, opts) end

            local value = pick("a", {})
            "#,
        );
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let result = infer_expr_type(&ws, file_id, LuaExpr::CallExpr(call_expr), true);
        assert!(matches!(result, Err(InferFailReason::None)));
    }

    #[test]
    fn test_no_flow_overload_resolution_declines_ambiguous_optional_tail_with_unknown_arg() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@overload fun(x: integer): integer
            ---@param x string
            ---@param y string?
            ---@return boolean
            local function pick(x, y) end

            local value = pick({})
            "#,
        );
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let result = infer_expr_type(&ws, file_id, LuaExpr::CallExpr(call_expr), true);
        assert!(matches!(result, Err(InferFailReason::None)));
    }

    #[test]
    fn test_no_flow_overload_resolution_uses_unknown_for_same_return_overloads() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@overload fun(x: string): boolean
            ---@param x integer
            ---@return boolean
            local function pick(x) end

            local value = pick({})
            "#,
        );
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let result = infer_expr_type(&ws, file_id, LuaExpr::CallExpr(call_expr), true);
        assert_eq!(result.unwrap(), ws.ty("boolean"));
    }

    #[test]
    fn test_no_flow_setmetatable_declines_unsupported_metatable_expr() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def("local t = {}\nlocal value = setmetatable(t, 1 + 2)\n");
        let call_expr = ws.get_node::<LuaCallExpr>(file_id);
        let result = infer_expr_type(&ws, file_id, LuaExpr::CallExpr(call_expr), true);
        assert!(matches!(result, Err(InferFailReason::None)));
    }

    #[test]
    fn test_infer_expr_no_flow_supports_table_const_index_with_union_key_expr() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            local t = {
                foo = 1,
                bar = 2,
            }

            ---@type 'foo' | 'bar'
            local k
            local value = t[k]
            "#,
        );
        let index_expr = ws.get_node::<LuaIndexExpr>(file_id);
        let expected = ws.ty("1 | 2");
        assert_infer_expr_type_matches(
            &ws,
            file_id,
            LuaExpr::IndexExpr(index_expr),
            &expected,
            true,
        );
    }

    #[test]
    fn test_infer_expr_no_flow_supports_inherited_custom_index_with_name_key_expr() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@class Base
            ---@field [integer] string

            ---@class Child: Base

            ---@type Child
            local child
            ---@type integer
            local i
            local value = child[i]
            "#,
        );
        let index_expr = ws.get_node::<LuaIndexExpr>(file_id);
        let expected = ws.ty("string");
        let result = infer_expr_type(&ws, file_id, LuaExpr::IndexExpr(index_expr), true);
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_infer_expr_no_flow_supports_table_generic_index_with_binary_key_expr() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@type table<integer, string>
            local map
            local value = map[1 + 2]
            "#,
        );
        let index_expr = ws.get_node::<LuaIndexExpr>(file_id);
        let expected = ws.ty("string");
        assert_infer_expr_type_matches(
            &ws,
            file_id,
            LuaExpr::IndexExpr(index_expr),
            &expected,
            true,
        );
    }
}
