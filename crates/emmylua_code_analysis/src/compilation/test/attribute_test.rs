#[cfg(test)]
mod test {
    use crate::VirtualWorkspace;

    #[test]
    fn test_def_attribute() {
        let mut ws = VirtualWorkspace::new();

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
        ---@[deprecated]
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
}
