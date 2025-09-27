#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_def_attribute() {
        let mut ws = VirtualWorkspace::new();
        ws.def(
            r#"
        ---@attribute Deprecated(message: string?)
        ---@attribute SkipDiagnosticTable() -- 跳过对表的部分诊断, 用于优化性能, 通常来说对巨型配置表使用.
        ---@attribute IndexFieldAlias(name: string) -- 索引字段别名, 将在`hint`与`completion`中显示别名.
        "#,
        );

        // ws.def(
        //     r#"
        // ---@attribute check_point(x: string, y: number)
        // "#,
        // );

        // ws.def(
        //     r#"
        // ---@attribute SkipDiagnosticTable()

        // ---@[SkipDiagnosticTable, Skip]
        // local config = {}
        // "#,
        // );

        ws.def(
            r#"
        ---@class A
        ---@field a string
        ---@[Deprecated]
        ---@field b string
        "#,
        );

        // ws.def(
        //     r#"

        // ---@[deprecated]
        // local a
        // "#,
        // );
    }

    #[test]
    fn test_attribute_attach() {
        let mut ws = VirtualWorkspace::new();
        // ws.def(
        //     r#"
        //     ---@generic [attribute] T
        //     local function f()
        //     end
        // "#,
        // );
        ws.def(
            r#"
            ---@generic [attribute] T
            local function f()
            end
        "#,
        );
    }
}
