#[cfg(test)]
mod tests {

    use crate::handlers::hover::test::{HoverVirtualWorkspace, VirtualHoverResult};
    #[test]
    fn test_1() {
        let mut ws = HoverVirtualWorkspace::new();
        assert!(ws.check_hover(
            r#"
                ---@class <??>A
                ---@field a number
                ---@field b string
                ---@field c boolean
            "#,
            VirtualHoverResult {
                value: "\n```lua\n(class) A {\n    a: number,\n    b: string,\n    c: boolean,\n}\n```\n\n\n".to_string(),
            },
        ));
    }
}
