#![allow(clippy::too_many_lines)]
#[cfg(test)]
mod tests {
    use crate::semantic_model::infer::narrow::{
        collect_dominating_conditions, narrow_local_at_point, narrow_remove_falsy, narrow_to_falsy,
    };
    use crate::{
        ConditionNodeId, Emmyrc, FileId, LuaType, SalsaDeclTreeSummary, SalsaFlowBranchLinkSummary,
        SalsaFlowConditionKindSummary, SalsaFlowConditionSummary, SalsaFlowEdgeKindSummary,
        SalsaFlowEdgeSummary, SalsaFlowNodeRefSummary, SalsaFlowQuerySummary, SalsaSummaryDatabase,
    };
    use emmylua_parser::{LuaAstNode, LuaExpr, LuaParser, LuaSyntaxId, ParserConfig};
    use rowan::TextSize;
    use std::path::PathBuf;
    use std::sync::Arc;

    // ═══════════════════════════════════════════════════════════════
    //  Unified test infrastructure
    // ═══════════════════════════════════════════════════════════════

    struct NarrowTester {
        db: SalsaSummaryDatabase,
        next_id: u32,
    }

    impl NarrowTester {
        fn new() -> Self {
            let mut db = SalsaSummaryDatabase::default();
            let mut emmyrc = Emmyrc::default();
            emmyrc.runtime.version = crate::config::EmmyrcLuaVersion::LuaJITExt;
            db.update_config(Arc::new(emmyrc));
            Self { db, next_id: 1 }
        }

        fn add(&mut self, src: &str) -> TestFile {
            let fid = self.next_id;
            self.next_id += 1;
            let file_id = FileId::new(fid);
            let path = format!("C:/ws/test{}.lua", fid);
            self.db
                .set_file(file_id, Some(PathBuf::from(path)), src.to_string(), false);
            let flow_query = self.db.flow().query(file_id).expect("flow query");
            let chunk = self
                .db
                .get_syntax_tree(file_id)
                .expect("syntax tree")
                .get_chunk_node();
            let decl_tree = self.db.file().decl_tree(file_id);
            TestFile {
                db: &self.db,
                file_id,
                flow_query,
                chunk,
                decl_tree,
            }
        }
    }

    struct TestFile<'a> {
        db: &'a SalsaSummaryDatabase,
        file_id: FileId,
        flow_query: Arc<SalsaFlowQuerySummary>,
        chunk: emmylua_parser::LuaChunk,
        decl_tree: Option<Arc<SalsaDeclTreeSummary>>,
    }

    impl<'a> TestFile<'a> {
        /// Narrow a variable at the first statement inside the if-true branch.
        fn narrow_in_if_true(&self, var: &str, base: LuaType) -> LuaType {
            let stmt = self.first_stmt_in_if_block();
            narrow_local_at_point(
                self.db,
                self.file_id,
                &self.chunk,
                stmt,
                var,
                base,
                self.decl_tree.as_deref(),
            )
        }

        /// Narrow a variable at the first statement inside the if-false (else) branch.
        fn narrow_in_if_false(&self, var: &str, base: LuaType) -> LuaType {
            let block = self
                .flow_query
                .branch_links
                .first()
                .and_then(|b| b.clause_block_offsets.get(1).copied())
                .expect("else clause block");
            let stmt = self.first_stmt_in(block);
            narrow_local_at_point(
                self.db,
                self.file_id,
                &self.chunk,
                stmt,
                var,
                base,
                self.decl_tree.as_deref(),
            )
        }

        /// Narrow a variable at the last statement in the whole file.
        fn narrow_at_last_stmt(&self, var: &str, base: LuaType) -> LuaType {
            let stmt = self
                .flow_query
                .edges
                .iter()
                .filter(|e| e.kind == SalsaFlowEdgeKindSummary::BlockToStatement)
                .last()
                .map(|e| match e.to {
                    SalsaFlowNodeRefSummary::Statement(s) => s,
                    _ => unreachable!(),
                })
                .expect("last stmt");
            narrow_local_at_point(
                self.db,
                self.file_id,
                &self.chunk,
                stmt,
                var,
                base,
                self.decl_tree.as_deref(),
            )
        }

        fn first_stmt_in_if_block(&self) -> TextSize {
            let block = self
                .flow_query
                .branch_links
                .first()
                .and_then(|b| b.clause_block_offsets.first().copied())
                .expect("if clause block");
            self.first_stmt_in(block)
        }

        fn first_stmt_in(&self, block: TextSize) -> TextSize {
            self.flow_query
                .edges
                .iter()
                .find(|e| {
                    e.kind == SalsaFlowEdgeKindSummary::BlockToStatement
                        && e.from == SalsaFlowNodeRefSummary::Block(block)
                })
                .map(|e| match e.to {
                    SalsaFlowNodeRefSummary::Statement(s) => s,
                    _ => unreachable!(),
                })
                .expect("first statement")
        }
    }

    // ═══════════════════════════════════════════════════════════════
    //  Type-level narrow tests
    // ═══════════════════════════════════════════════════════════════

    fn ty(types: Vec<LuaType>) -> LuaType {
        LuaType::from_vec(types)
    }

    #[test]
    fn test_narrow_remove_falsy_removes_nil() {
        assert_eq!(
            narrow_remove_falsy(ty(vec![LuaType::String, LuaType::Nil])),
            LuaType::String
        );
    }

    #[test]
    fn test_narrow_to_falsy_keeps_nil() {
        assert_eq!(
            narrow_to_falsy(ty(vec![LuaType::String, LuaType::Nil])),
            LuaType::Nil
        );
    }

    #[test]
    fn test_narrow_remove_falsy_bool() {
        assert_eq!(
            narrow_remove_falsy(LuaType::Boolean),
            LuaType::BooleanConst(true)
        );
    }

    #[test]
    fn test_narrow_to_falsy_bool() {
        assert_eq!(
            narrow_to_falsy(LuaType::Boolean),
            LuaType::BooleanConst(false)
        );
    }

    #[test]
    fn test_narrow_remove_falsy_bool_false_is_unknown() {
        assert_eq!(
            narrow_remove_falsy(LuaType::BooleanConst(false)),
            LuaType::Unknown
        );
    }

    #[test]
    fn test_narrow_to_falsy_str_is_never() {
        assert_eq!(narrow_to_falsy(LuaType::String), LuaType::Never);
    }

    // ═══════════════════════════════════════════════════════════════
    //  Condition analysis unit tests
    // ═══════════════════════════════════════════════════════════════

    fn parse_chunk(code: &str) -> emmylua_parser::LuaChunk {
        LuaParser::parse(code, ParserConfig::default()).get_chunk_node()
    }

    fn parse_expr(code: &str) -> LuaExpr {
        let tree = LuaParser::parse(&format!("local _ = {}", code), ParserConfig::default());
        for expr in tree.get_chunk_node().descendants::<LuaExpr>() {
            if u32::from(expr.syntax().text_range().start()) > 0 {
                return expr;
            }
        }
        panic!("parse: {code}");
    }

    fn cond_effects(code: &str, var: &str, is_true: bool) -> Vec<&'static str> {
        let chunk = parse_chunk(&format!("local _ = {}", code));
        let expr = parse_expr(code);
        let cond = SalsaFlowConditionSummary {
            node_offset: ConditionNodeId(1),
            syntax_offset: expr.syntax().text_range().start(),
            syntax_id: LuaSyntaxId::from_node(expr.syntax()),
            kind: SalsaFlowConditionKindSummary::Expr,
            left_condition_offset: None,
            right_condition_offset: None,
        };
        super::super::collect_leaf_conditions(&[cond], &chunk, ConditionNodeId(1), var, is_true)
            .iter()
            .map(|e| match e {
                super::super::ConditionEffect::Truthy => "truthy",
                super::super::ConditionEffect::Falsy => "falsy",
                super::super::ConditionEffect::EqLiteral(_) => "eq",
                super::super::ConditionEffect::NeqLiteral(_) => "neq",
                super::super::ConditionEffect::TypeGuard(_) => "guard",
            })
            .collect()
    }

    macro_rules! assert_cond {
        ($code:expr, $var:expr, $branch:expr, [ $($expected:expr),* ]) => {
            assert_eq!(
                cond_effects($code, $var, $branch),
                vec![$($expected),*],
                "cond: {} branch={} var={}", $code, stringify!($branch), $var
            );
        };
    }

    #[test]
    fn test_cond_patterns() {
        assert_cond!("x", "x", true, ["truthy"]);
        assert_cond!("x", "x", false, ["falsy"]);
        assert_cond!("not x", "x", true, ["falsy"]);
        assert_cond!("not x", "x", false, ["truthy"]);
        assert_cond!(r#"x == "hello""#, "x", true, ["eq"]);
        assert_cond!(r#"x ~= "hello""#, "x", true, ["neq"]);
        assert_cond!("x == nil", "x", true, ["eq"]);
        assert_cond!("x ~= nil", "x", true, ["neq"]);
        assert_cond!(r#"type(x) == "string""#, "x", true, ["guard"]);
        assert_cond!(r#"type(x) == "string""#, "x", false, ["falsy"]);
        assert_cond!("#xs > 0", "xs", true, ["truthy"]);
        assert_cond!("obj.field", "obj", true, ["truthy"]);
        assert_cond!("not obj.field", "obj", false, ["truthy"]);
        assert_cond!("x > 0", "x", true, ["truthy"]);
        assert_cond!(r#""hello" == x"#, "x", true, ["eq"]);
        assert_cond!("10 > x", "x", true, ["truthy"]);
        // Non-matching var produces empty effects
        assert!(cond_effects(r#"y == "hello""#, "x", true).is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    //  Flow-collection unit tests
    // ═══════════════════════════════════════════════════════════════

    fn edge_bts(block: u32, stmt: u32) -> SalsaFlowEdgeSummary {
        SalsaFlowEdgeSummary {
            kind: SalsaFlowEdgeKindSummary::BlockToStatement,
            from: SalsaFlowNodeRefSummary::Block(TextSize::from(block)),
            to: SalsaFlowNodeRefSummary::Statement(TextSize::from(stmt)),
        }
    }
    fn edge_ct(cond: u32, block: u32) -> SalsaFlowEdgeSummary {
        SalsaFlowEdgeSummary {
            kind: SalsaFlowEdgeKindSummary::ConditionTrue,
            from: SalsaFlowNodeRefSummary::Condition(ConditionNodeId(cond)),
            to: SalsaFlowNodeRefSummary::Block(TextSize::from(block)),
        }
    }
    fn edge_cf(cond: u32, block: u32) -> SalsaFlowEdgeSummary {
        SalsaFlowEdgeSummary {
            kind: SalsaFlowEdgeKindSummary::ConditionFalse,
            from: SalsaFlowNodeRefSummary::Condition(ConditionNodeId(cond)),
            to: SalsaFlowNodeRefSummary::Block(TextSize::from(block)),
        }
    }
    fn edge_btc(branch: u32, block: u32) -> SalsaFlowEdgeSummary {
        SalsaFlowEdgeSummary {
            kind: SalsaFlowEdgeKindSummary::BranchToClause,
            from: SalsaFlowNodeRefSummary::Branch(TextSize::from(branch)),
            to: SalsaFlowNodeRefSummary::Block(TextSize::from(block)),
        }
    }
    fn branch(b: u32, e: u32, clauses: Vec<u32>) -> SalsaFlowBranchLinkSummary {
        SalsaFlowBranchLinkSummary {
            branch_offset: TextSize::from(b),
            entry_block_offset: Some(TextSize::from(e)),
            clause_block_offsets: clauses.into_iter().map(TextSize::from).collect(),
            exit_block_offset: Some(TextSize::from(e)),
        }
    }

    #[test]
    fn test_flow_if_branch_collects_condition() {
        let q = SalsaFlowQuerySummary {
            root_block_offsets: vec![],
            branch_links: vec![branch(15, 0, vec![20])],
            loop_links: vec![],
            return_links: vec![],
            break_links: vec![],
            continue_links: vec![],
            goto_links: vec![],
            terminal_edges: vec![],
            edges: vec![
                edge_bts(0, 10),
                edge_bts(20, 30),
                edge_ct(1, 20),
                edge_btc(15, 20),
            ],
        };
        let n = collect_dominating_conditions(&q, TextSize::from(30u32));
        assert_eq!(n.len(), 1);
        assert!(n[0].is_true_branch);
        assert_eq!(n[0].condition_offset, ConditionNodeId(1));
    }

    #[test]
    fn test_flow_else_branch_collects_condition() {
        let q = SalsaFlowQuerySummary {
            root_block_offsets: vec![],
            branch_links: vec![branch(15, 0, vec![20, 30])],
            loop_links: vec![],
            return_links: vec![],
            break_links: vec![],
            continue_links: vec![],
            goto_links: vec![],
            terminal_edges: vec![],
            edges: vec![
                edge_bts(0, 10),
                edge_bts(30, 40),
                edge_cf(1, 30),
                edge_btc(15, 30),
            ],
        };
        let n = collect_dominating_conditions(&q, TextSize::from(40u32));
        assert_eq!(n.len(), 1);
        assert!(!n[0].is_true_branch);
    }

    #[test]
    fn test_flow_no_conditions_for_linear_code() {
        let q = SalsaFlowQuerySummary {
            root_block_offsets: vec![],
            branch_links: vec![],
            loop_links: vec![],
            return_links: vec![],
            break_links: vec![],
            continue_links: vec![],
            goto_links: vec![],
            terminal_edges: vec![],
            edges: vec![edge_bts(0, 10), edge_bts(0, 20)],
        };
        assert!(collect_dominating_conditions(&q, TextSize::from(20u32)).is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    //  End-to-end tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_e2e_if_x_then_narrows_to_non_nil() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local x = nil
            if x then
                local r = x
            end
        "#,
        );
        let r = f.narrow_in_if_true("x", ty(vec![LuaType::String, LuaType::Nil]));
        assert_eq!(r, LuaType::String, "if x then: x should be string");
    }

    #[test]
    fn test_e2e_if_x_else_narrows_to_nil() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local x = nil
            if x then
                local r1 = x
            else
                local r2 = x
            end
        "#,
        );
        let r = f.narrow_in_if_false("x", ty(vec![LuaType::String, LuaType::Nil]));
        assert_eq!(r, LuaType::Nil, "if x else: x should be nil");
    }

    #[test]
    fn test_e2e_if_not_x_then_narrows_to_nil() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local x = nil
            if not x then
                local r = x
            end
        "#,
        );
        let r = f.narrow_in_if_true("x", ty(vec![LuaType::String, LuaType::Nil]));
        assert_eq!(r, LuaType::Nil, "if not x then: x should be nil");
    }

    #[test]
    fn test_e2e_if_x_neq_nil_then_narrows_to_string() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local x = nil
            if x ~= nil then
                local r = x
            end
        "#,
        );
        let r = f.narrow_in_if_true("x", ty(vec![LuaType::String, LuaType::Nil]));
        assert_eq!(r, LuaType::String, "x ~= nil: should be string");
    }

    #[test]
    fn test_e2e_type_guard_narrows_to_type() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local x = nil
            if type(x) == "string" then
                local r = x
            end
        "#,
        );
        let r = f.narrow_in_if_true(
            "x",
            ty(vec![LuaType::String, LuaType::Integer, LuaType::Nil]),
        );
        assert_eq!(r, LuaType::String, "type(x)=='string': should be string");
    }

    #[test]
    fn test_e2e_len_gt_0_narrows_to_truthy() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local xs = nil
            if #xs > 0 then
                local r = xs
            end
        "#,
        );
        let r = f.narrow_in_if_true("xs", ty(vec![LuaType::String, LuaType::Nil]));
        assert_eq!(r, LuaType::String, "#xs > 0: should be non-nil");
    }

    #[test]
    fn test_e2e_no_condition_unchanged() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local x = nil
            local r = x
        "#,
        );
        let base = ty(vec![LuaType::String, LuaType::Nil]);
        assert_eq!(f.narrow_at_last_stmt("x", base.clone()), base);
    }

    // ═══════════════════════════════════════════════════════════════
    //  return_overload tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_return_overload_discriminant_true_filters_to_row0() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            ---@return_overload true, string
            ---@return_overload false, integer
            local function f(x) return true, x end

            local ok, value = f("hello")
            if ok then
                local r = value
            end
        "#,
        );
        // ok is truthy → matches "true" row → value at slot 1 = string
        let r = f.narrow_in_if_true("value", ty(vec![LuaType::String, LuaType::Integer]));
        assert_eq!(r, LuaType::String, "ok=true: value should be string");
    }

    #[test]
    fn test_return_overload_discriminant_false_filters_to_row1() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            ---@return_overload true, string
            ---@return_overload false, integer
            local function f(x) return false, x end

            local ok, value = f(1)
            if not ok then
                local r = value
            end
        "#,
        );
        // not ok (false branch of `if not ok`) → ok is truthy → wait...
        // `if not ok then` → in the true branch, not-ok is true, so ok is false
        // That's the `narrow_in_if_true` path with var "value"
        // But we're in the TRUE branch of `if not ok`, meaning ok is FALSE
        // So matches "false" row → value at slot 1 = integer
        let r = f.narrow_in_if_true("value", ty(vec![LuaType::String, LuaType::Integer]));
        assert_eq!(r, LuaType::Integer, "not ok: value should be integer");
    }

    #[test]
    fn test_return_overload_no_discriminant_condition_returns_union() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            ---@return_overload true, string
            ---@return_overload false, integer
            local function f(x) return true, x end

            local ok, value = f("hello")
            local r = value
        "#,
        );
        // No condition on ok → all overload rows match → value = string | integer
        let r = f.narrow_at_last_stmt("value", ty(vec![LuaType::String, LuaType::Integer]));
        assert_eq!(
            r,
            ty(vec![LuaType::String, LuaType::Integer]),
            "no condition: value should be string|integer"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    //  Inference integration tests
    // ═══════════════════════════════════════════════════════════════

    use crate::semantic_model::SemanticModel;
    use smol_str::SmolStr;

    #[test]
    fn test_integration_infer_literal_and_narrow() {
        let mut t = NarrowTester::new();
        let f = t.add(
            r#"
            local x = "hello"
            if x then
                local r = x
            end
        "#,
        );
        let model = SemanticModel::new(
            f.file_id,
            f.db,
            Arc::new(Emmyrc::default()),
            f.chunk.clone(),
        );
        // Find the `x` inside `local r = x` in the if-branch
        let x_ref = model
            .root
            .descendants::<emmylua_parser::LuaNameExpr>()
            .filter(|n| n.get_name_token().is_some_and(|t| t.get_name_text() == "x"))
            .last()
            .expect("x reference");
        let result = model.infer_expr(LuaExpr::NameExpr(x_ref));
        let ty = result.expect("infer_expr ok");
        // x = "hello" → StringConst; if x then → narrow_remove_falsy → same (truthy)
        assert_eq!(
            ty,
            LuaType::StringConst(SmolStr::new("hello").into()),
            "x should be StringConst, got {:?}",
            ty
        );
    }
}
