#[cfg(test)]
mod test {
    use emmylua_parser::{LuaAstNode, LuaCallExpr, LuaClosureExpr, LuaIndexExpr, LuaLocalStat};

    use crate::{
        LuaSemanticDeclId, LuaSignatureId, LuaType, SalsaSyntaxIdSummary, SemanticDeclLevel,
        VirtualWorkspace,
    };

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
            ---@field e fun(a: string, b: boolean)
            ---@field e fun(a: number, b: boolean)
            local Test

            function Test.e(a, b)
            end
            A = Test.e
            "#,
        );
        // 必须要这样写, 无法直接`A = a`拿到`a`的实际类型, `A`的推断目前是独立的且在`Test.e`推断之前缓存
        let ty = ws.expr_ty("A");
        let expected_a = ws.ty("string|number");
        // let expected_a_str = ws.humanize_type(expected_a);

        match ty {
            LuaType::Union(union) => {
                let types = union.into_vec();
                let signature = types
                    .iter()
                    .last()
                    .and_then(|t| match t {
                        LuaType::Signature(id) => {
                            ws.get_db_mut().get_signature_index_mut().get_mut(id)
                        }
                        _ => None,
                    })
                    .expect("Expected a function type");

                let param_type = signature
                    .get_param_info_by_name("a")
                    .map(|p| p.type_ref.clone())
                    .expect("Parameter 'a' not found");

                assert_eq!(param_type, expected_a);
            }
            _ => panic!("Expected a union type"),
        }
    }

    #[test]
    fn test_field_doc_function_2() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
            ---@class ClosureTest
            local Test

            ---@overload fun(a: string, b: number)
            ---@overload fun(a: number, b: number)
            function Test.e(a, b)
                A = a
                B = b
            end
            "#,
        );

        {
            let ty = ws.expr_ty("A");
            let expected = ws.ty("string|number");
            assert_eq!(ty, expected);
        }

        {
            let ty = ws.expr_ty("B");
            let expected = ws.ty("number");
            assert_eq!(ty, expected);
        }
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

    #[test]
    fn test_issue_416() {
        let mut ws = VirtualWorkspace::new();
        ws.def_files(vec![
            (
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
                    B = self.custom_event_manager
                    local trigger = self.custom_event_manager:get_trigger()
                    A = trigger
                    return trigger
                end
            "#,
            ),
            (
                "test2.lua",
                r#"
                require "test1"
                ---@class Trigger

                ---@class EventManager
                local EventManager

                ---@return Trigger
                function EventManager:get_trigger()
                end
            "#,
            ),
        ]);

        let ty = ws.expr_ty("A");
        let expected = ws.ty("Trigger");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_field_doc_function_4() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Trigger.CallBack fun(trg: Trigger, ...): any, any, any, any

                ---@class CustomEvent1
                ---@field event_on fun(self: self, event_name:string, callback:Trigger.CallBack):Trigger
                ---@field event_on fun(self: self, event_name:string, args:any[] | any, callback:Trigger.CallBack):Trigger
                local M


                function M:event_on(...)
                    local event_name, args, callback = ...
                    A = args
                end

            "#,
        );
        let ty = ws.expr_ty("A");
        let expected = ws.ty("any");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_field_doc_function_5() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
                ---@alias Trigger.CallBack fun(trg: Trigger, ...): any, any, any, any

                ---@class CustomEvent1
                local M

                ---@overload fun(self: self, event_name:string, callback:Trigger.CallBack):Trigger
                ---@overload fun(self: self, event_name:string, args:any[] | any, callback:Trigger.CallBack):Trigger
                function M:event_on(...)
                    local event_name, args, callback = ...
                    A = args
                end

            "#,
        );
        let ty = ws.expr_ty("A");
        let expected = ws.ty("any");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_issue_498() {
        let mut ws = VirtualWorkspace::new();
        ws.def_files(vec![
            (
                "test.lua",
                r#"
                ---@class CustomEvent
                ---@field private custom_event_manager? EventManager
                local M = {}

                function M:event_on()
                    if not self.custom_event_manager then
                        self.custom_event_manager = New 'EventManager' (self)
                    end
                    local trigger = self.custom_event_manager:get_trigger()
                    A = trigger
                    return trigger
                end
            "#,
            ),
            (
                "test2.lua",
                r#"
                ---@class Trigger

                ---@class EventManager
                ---@overload fun(object?: table): self
                local EventManager

                ---@return Trigger
                function EventManager:get_trigger()
                end
            "#,
            ),
            (
                "class.lua",
                r#"
                local M = {}

                ---@generic T: string
                ---@param name `T`
                ---@param tbl? table
                ---@return T
                function M.declare(name, tbl)
                end
                return M
            "#,
            ),
            (
                "init.lua",
                r#"
                New = require "class".declare
            "#,
            ),
        ]);
        let ty = ws.expr_ty("A");
        let expected = ws.ty("Trigger");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_param_function_is_alias() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@class LocalTimer
            ---@alias LocalTimer.OnTimer fun(timer: LocalTimer, count: integer, ...: any)

            ---@param on_timer LocalTimer.OnTimer
            ---@return LocalTimer
            function loop_count(on_timer)
            end

            loop_count(function(timer, count)
                A = timer
            end)
            "#,
        );
        let ty = ws.expr_ty("A");
        let expected = ws.ty("LocalTimer");
        assert_eq!(ws.humanize_type(ty), ws.humanize_type(expected));
    }

    #[test]
    fn test_issue_791() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
            ---@alias HookAlias fun(a:integer)

            ---@class TypeA
            ---@field hook HookAlias

            ---@class TypeB
            ---@field hook fun(a:integer)

            ---@param d TypeA
            function fnA(d) end

            ---@param d TypeB
            function fnB(d) end

            fnA({ hook = function(obj) a = obj end }) -- obj is any, not integer
            "#,
        );
        let ty = ws.expr_ty("a");
        let expected = ws.ty("integer");
        assert_eq!(ty, expected);
    }

    #[test]
    fn test_semantic_model_call_and_signature_explain_bridge() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@class Box
            local Box = {}

            ---@param value integer
            ---@return integer
            function Box.run(value)
                return value
            end

            local result = Box.run(1)
            "#,
        );

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("semantic model");
        let call_expr = semantic_model
            .get_root()
            .descendants::<LuaCallExpr>()
            .last()
            .expect("call expr");

        let call_explain = semantic_model
            .call_explain(call_expr.clone())
            .expect("call explain");
        let resolved_signature_id = semantic_model
            .resolved_call_signature_id(call_expr.clone())
            .expect("resolved signature id");
        let signature_summary = semantic_model
            .signature_summary(resolved_signature_id)
            .expect("signature summary");
        let signature_explain = signature_summary.explain.clone();

        assert_eq!(
            resolved_signature_id,
            LuaSignatureId::new(
                file_id,
                call_explain
                    .resolved_signature_offset
                    .expect("resolved signature offset"),
            )
        );
        assert_eq!(signature_explain.signature.name.as_deref(), Some("Box.run"));
        assert_eq!(call_explain.args.len(), 1);
        assert!(matches!(
            call_explain.args[0].expected_param,
            Some(crate::SalsaSignatureParamExplainSummary { ref name, .. }) if name == "value"
        ));
        assert_eq!(signature_summary.signature.name.as_deref(), Some("Box.run"));
        assert_eq!(signature_explain.params.len(), 1);
        assert!(matches!(
            signature_explain.params[0].doc_type,
            Some(crate::SalsaSignatureTypeExplainSummary {
                lowered: Some(crate::SalsaDocTypeLoweredNode {
                    kind: crate::SalsaDocTypeLoweredKind::Name { ref name },
                    ..
                }),
                ..
            }) if name == "integer"
        ));
    }

    #[test]
    fn test_semantic_model_decl_summary_bridge() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            local value = 1
            local copy = value
            "#,
        );

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("semantic model");
        let value_name = semantic_model
            .get_root()
            .descendants::<LuaLocalStat>()
            .next()
            .and_then(|stat| stat.get_local_name_list().next())
            .expect("value local name");

        let semantic_decl = semantic_model
            .find_decl(
                value_name.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )
            .expect("semantic decl");
        let decl_id = match semantic_decl {
            LuaSemanticDeclId::LuaDecl(decl_id) => decl_id,
            other => panic!("expected lua decl, got {other:?}"),
        };
        let decl_summary = semantic_model.decl_summary(decl_id).expect("decl summary");
        let host_decl_summary = semantic_model
            .get_summary()
            .semantic()
            .file()
            .decl_summary_by_syntax_id(file_id, value_name.get_syntax_id().into())
            .expect("host decl summary");

        assert_eq!(
            decl_summary.decl_type.decl_id.as_position(),
            decl_id.position
        );
        assert_eq!(decl_summary.decl_type.name.as_str(), "value");
        assert_eq!(decl_summary, host_decl_summary);
    }

    #[test]
    fn test_semantic_model_member_summary_bridge() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            local Box = {}
            Box.value = 1
            local copy = Box.value
            "#,
        );

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("semantic model");
        let value_index = semantic_model
            .get_root()
            .descendants::<LuaIndexExpr>()
            .last()
            .expect("value index expr");

        let semantic_decl = semantic_model
            .find_decl(
                value_index.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )
            .expect("semantic decl");
        let member_id = match semantic_decl {
            LuaSemanticDeclId::Member(member_id) => member_id,
            other => panic!("expected member decl, got {other:?}"),
        };
        let member_summary = semantic_model
            .member_summary(member_id)
            .expect("member summary");
        let host_member_summary = semantic_model
            .get_summary()
            .semantic()
            .file()
            .member_summary_by_syntax_id(
                file_id,
                crate::SalsaSyntaxIdSummary::from(*member_id.get_syntax_id()),
            )
            .expect("host member summary");

        assert_eq!(
            member_summary.member_type.target.member_name.as_str(),
            "value"
        );
        assert_eq!(member_summary, host_member_summary);
    }

    #[test]
    fn test_semantic_model_member_owner_type_bridge() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@class Box
            local Box = {}
            function Box:get() end
            local copy = Box.get
            "#,
        );

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("semantic model");
        let holder_value = semantic_model
            .get_root()
            .descendants::<LuaIndexExpr>()
            .last()
            .expect("box get index expr");

        let semantic_decl = semantic_model
            .find_decl(
                holder_value.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )
            .expect("semantic decl");
        let member_id = match semantic_decl {
            LuaSemanticDeclId::Member(member_id) => member_id,
            other => panic!("expected member decl, got {other:?}"),
        };

        let owner_type = semantic_model
            .infer_member_access_owner_type(member_id)
            .expect("member owner type");
        let box_type = ws.ty("Box");

        assert_eq!(ws.humanize_type(owner_type), ws.humanize_type(box_type));
    }

    #[test]
    fn test_semantic_model_infer_bind_value_type_prefers_summary_expected_closure_type() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@param cb fun(value: integer): string
            local function call(cb)
            end

            call(function(value)
                return tostring(value)
            end)
            "#,
        );

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("semantic model");
        let closure = semantic_model
            .get_root()
            .descendants::<LuaClosureExpr>()
            .last()
            .expect("closure expr");
        let property = semantic_model
            .get_summary()
            .file()
            .property_by_value_expr_syntax_id(
                file_id,
                SalsaSyntaxIdSummary::from(closure.get_syntax_id()),
            );
        assert!(
            property.is_some(),
            "expected property for closure table field value"
        );

        let local_name = semantic_model
            .get_root()
            .descendants::<LuaLocalStat>()
            .next()
            .and_then(|stat| stat.get_local_name_list().next())
            .expect("local name");
        let decl = semantic_model
            .find_decl(
                local_name.syntax().clone().into(),
                SemanticDeclLevel::default(),
            )
            .expect("semantic decl");
        let LuaSemanticDeclId::LuaDecl(decl_id) = decl else {
            panic!("expected lua decl, got {decl:?}");
        };
        let decl_summary = semantic_model.decl_summary(decl_id).expect("decl summary");
        println!(
            "decl candidate type offsets: {:?}",
            decl_summary.value_shell.candidate_type_offsets
        );
        assert!(
            !decl_summary.value_shell.candidate_type_offsets.is_empty(),
            "expected local decl value shell to carry explicit @type"
        );

        let merged_member = semantic_model.get_compilation().find_type_merged_member(
            file_id,
            "Completion2.A",
            semantic_model.get_compilation().legacy_db().resolve_workspace_id(file_id),
            "event",
        );
        println!("merged member exists: {}", merged_member.is_some());
        let merged_member = merged_member.expect("expected compilation merged member");
        println!("merged member source: {:?}", merged_member.source);
        println!(
            "merged member syntax offset: {:?}",
            merged_member.syntax_offset
        );
        let summary_property = merged_member.syntax_offset.and_then(|syntax_offset| {
            semantic_model
                .get_summary()
                .file()
                .property_at(merged_member.file_id, syntax_offset)
        });
        println!("summary property exists: {}", summary_property.is_some());
        let summary_property =
            summary_property.expect("expected summary property for merged member");
        println!(
            "summary property doc_type_offset: {:?}",
            summary_property.doc_type_offset
        );
        println!(
            "summary property value_expr_offset: {:?}",
            summary_property.value_expr_offset
        );
        assert!(
            summary_property.doc_type_offset.is_some()
                || summary_property.value_expr_offset.is_some(),
            "expected summary property to carry doc type or value expr"
        );

        let typ = semantic_model
            .infer_bind_value_type(closure.clone().into())
            .expect("expected bind value type");

        assert_eq!(ws.humanize_type(typ), "fun(value: integer) -> string");
    }

    #[test]
    fn test_semantic_model_infer_bind_value_type_for_type_annotated_table_field_closure() {
        let mut ws = VirtualWorkspace::new();
        let file_id = ws.def(
            r#"
            ---@class Completion2.A
            ---@field event fun(aaa): integer

            ---@type Completion2.A
            local a = {
                event = function(aaa)
                    return aaa
                end,
            }
            "#,
        );

        let semantic_model = ws
            .analysis
            .compilation
            .get_semantic_model(file_id)
            .expect("semantic model");
        let closure = semantic_model
            .get_root()
            .descendants::<LuaClosureExpr>()
            .last()
            .expect("closure expr");

        let typ = semantic_model
            .infer_bind_value_type(closure.clone().into())
            .expect("expected bind value type");

        assert_eq!(ws.humanize_type(typ), "fun(aaa) -> integer");
    }
}
