#[cfg(test)]
mod tests {
    use crate::config::LuaFormatConfig;
    use crate::ir::*;
    use crate::printer::Printer;

    #[test]
    fn test_simple_text() {
        let config = LuaFormatConfig::default();
        let printer = Printer::new(&config);
        let docs = vec![text("hello"), space(), text("world")];
        let result = printer.print(&docs);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_hard_line() {
        let config = LuaFormatConfig::default();
        let printer = Printer::new(&config);
        let docs = vec![text("line1"), hard_line(), text("line2")];
        let result = printer.print(&docs);
        assert_eq!(result, "line1\nline2");
    }

    #[test]
    fn test_group_flat() {
        let config = LuaFormatConfig::default();
        let printer = Printer::new(&config);
        let docs = vec![group(vec![
            text("f("),
            soft_line_or_empty(),
            text("a"),
            text(","),
            soft_line(),
            text("b"),
            soft_line_or_empty(),
            text(")"),
        ])];
        let result = printer.print(&docs);
        assert_eq!(result, "f(a, b)");
    }

    #[test]
    fn test_group_break() {
        let config = LuaFormatConfig {
            layout: crate::config::LayoutConfig {
                max_line_width: 10,
                ..Default::default()
            },
            ..Default::default()
        };
        let printer = Printer::new(&config);
        let docs = vec![group(vec![
            text("f("),
            indent(vec![
                soft_line_or_empty(),
                text("very_long_arg1"),
                text(","),
                soft_line(),
                text("very_long_arg2"),
            ]),
            soft_line_or_empty(),
            text(")"),
        ])];
        let result = printer.print(&docs);
        assert_eq!(result, "f(\n    very_long_arg1,\n    very_long_arg2\n)");
    }

    #[test]
    fn test_indent() {
        let config = LuaFormatConfig::default();
        let printer = Printer::new(&config);
        let docs = vec![
            text("if true then"),
            indent(vec![hard_line(), text("print(1)")]),
            hard_line(),
            text("end"),
        ];
        let result = printer.print(&docs);
        assert_eq!(result, "if true then\n    print(1)\nend");
    }
}
