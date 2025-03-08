#[cfg(test)]
mod tests {
    use crate::{TypeOps, VirtualWorkspace};

    #[test]
    fn test_custom_ops() {
        let mut ws = VirtualWorkspace::new();

        ws.def(
            r#"
        ---@class a
        ---@class b
        "#,
        );

        assert_eq!(
            TypeOps::Union.apply(&ws.ty("a"), &ws.ty("b")),
            ws.ty("a | b")
        );
        assert_eq!(
            TypeOps::Union.apply(&ws.ty("a | b"), &ws.ty("string")),
            ws.ty("a | b | string")
        );

        assert_eq!(
            TypeOps::Remove.apply(&ws.ty("a | b"), &ws.ty("a")),
            ws.ty("b")
        );
        assert_eq!(
            TypeOps::Remove.apply(&ws.ty("a?"), &ws.ty("nil")),
            ws.ty("a")
        );
        assert_eq!(
            TypeOps::Remove.apply(&ws.ty("a | nil"), &ws.ty("nil")),
            ws.ty("a")
        );

        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("a | b"), &ws.ty("a")),
            ws.ty("a")
        );
        assert_eq!(TypeOps::Narrow.apply(&ws.ty("a?"), &ws.ty("a")), ws.ty("a"));
        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("a | b"), &ws.ty("a | b")),
            ws.ty("a | b")
        );
    }

    #[test]
    fn test_basic() {
        let mut ws = VirtualWorkspace::new();

        assert_eq!(
            TypeOps::Union.apply(&ws.ty("string"), &ws.ty("'ssss'")),
            ws.ty("string")
        );
        assert_eq!(
            TypeOps::Union.apply(&ws.ty("string"), &ws.ty("number")),
            ws.ty("string | number")
        );
        assert_eq!(
            TypeOps::Union.apply(&ws.ty("number"), &ws.ty("integer")),
            ws.ty("number")
        );
        assert_eq!(
            TypeOps::Union.apply(&ws.ty("integer"), &ws.ty("1")),
            ws.ty("integer")
        );
        assert_eq!(TypeOps::Union.apply(&ws.ty("1"), &ws.ty("2")), ws.ty("1|2"));

        assert_eq!(
            TypeOps::Remove.apply(&ws.ty("string | number"), &ws.ty("string")),
            ws.ty("number")
        );

        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("string | number"), &ws.ty("string")),
            ws.ty("string")
        );

        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("string | number"), &ws.ty("number")),
            ws.ty("number")
        );

        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("string | nil"), &ws.ty("string")),
            ws.ty("string")
        );

        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("number | nil"), &ws.ty("number")),
            ws.ty("number")
        );

        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("1 | nil"), &ws.ty("integer")),
            ws.ty("1")
        );

        assert_eq!(
            TypeOps::Narrow.apply(&ws.ty("string[]?"), &ws.expr_ty("{}")),
            ws.ty("string[]")
        );
    }
}
