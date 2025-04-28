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
        {
            let type_a = ws.ty("a");
            let type_b = ws.ty("b");
            assert_eq!(
                TypeOps::Union.apply(ws.get_db_mut(), &type_a, &type_b),
                ws.ty("a | b")
            );
        }
        {
            let type_ab = ws.ty("a | b");
            let type_string = ws.ty("string");
            assert_eq!(
                TypeOps::Union.apply(ws.get_db_mut(), &type_ab, &type_string),
                ws.ty("a | b | string")
            );
        }
        {
            let type_ab = ws.ty("a | b");
            let type_a = ws.ty("a");
            assert_eq!(
                TypeOps::Remove.apply(ws.get_db_mut(), &type_ab, &type_a),
                ws.ty("b")
            );
        }
        {
            let type_a_opt = ws.ty("a?");
            let type_nil = ws.ty("nil");
            assert_eq!(
                TypeOps::Remove.apply(ws.get_db_mut(), &type_a_opt, &type_nil),
                ws.ty("a")
            );
        }
        {
            let type_a_nil = ws.ty("a | nil");
            let type_nil = ws.ty("nil");
            assert_eq!(
                TypeOps::Remove.apply(ws.get_db_mut(), &type_a_nil, &type_nil),
                ws.ty("a")
            );
        }
        {
            let type_ab = ws.ty("a | b");
            let type_a = ws.ty("a");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_ab, &type_a),
                ws.ty("a")
            );
        }
        {
            let type_a_opt = ws.ty("a?");
            let type_a = ws.ty("a");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_a_opt, &type_a),
                ws.ty("a")
            );
        }
        {
            let type_ab = ws.ty("a | b");
            let type_ab2 = ws.ty("a | b");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_ab, &type_ab2),
                ws.ty("a | b")
            );
        }
    }

    #[test]
    fn test_basic() {
        let mut ws = VirtualWorkspace::new();

        {
            let type_string = ws.ty("string");
            let type_literal = ws.ty("'ssss'");
            assert_eq!(
                TypeOps::Union.apply(ws.get_db_mut(), &type_string, &type_literal),
                ws.ty("string")
            );
        }
        {
            let type_string = ws.ty("string");
            let type_number = ws.ty("number");
            assert_eq!(
                TypeOps::Union.apply(ws.get_db_mut(), &type_string, &type_number),
                ws.ty("string | number")
            );
        }
        {
            let type_number = ws.ty("number");
            let type_integer = ws.ty("integer");
            assert_eq!(
                TypeOps::Union.apply(ws.get_db_mut(), &type_number, &type_integer),
                ws.ty("number")
            );
        }
        {
            let type_integer = ws.ty("integer");
            let type_one = ws.ty("1");
            assert_eq!(
                TypeOps::Union.apply(ws.get_db_mut(), &type_integer, &type_one),
                ws.ty("integer")
            );
        }
        {
            let type_one = ws.ty("1");
            let type_two = ws.ty("2");
            assert_eq!(
                TypeOps::Union.apply(ws.get_db_mut(), &type_one, &type_two),
                ws.ty("1|2")
            );
        }
        {
            let type_string_number = ws.ty("string | number");
            let type_string = ws.ty("string");
            assert_eq!(
                TypeOps::Remove.apply(ws.get_db_mut(), &type_string_number, &type_string),
                ws.ty("number")
            );
        }
        {
            let type_string_number = ws.ty("string | number");
            let type_string = ws.ty("string");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_string_number, &type_string),
                ws.ty("string")
            );
        }
        {
            let type_string_number = ws.ty("string | number");
            let type_number = ws.ty("number");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_string_number, &type_number),
                ws.ty("number")
            );
        }
        {
            let type_string_nil = ws.ty("string | nil");
            let type_string = ws.ty("string");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_string_nil, &type_string),
                ws.ty("string")
            );
        }
        {
            let type_number_nil = ws.ty("number | nil");
            let type_number = ws.ty("number");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_number_nil, &type_number),
                ws.ty("number")
            );
        }
        {
            let type_one_nil = ws.ty("1 | nil");
            let type_integer = ws.ty("integer");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_one_nil, &type_integer),
                ws.ty("1")
            );
        }
        {
            let type_string_array_opt = ws.ty("string[]?");
            let type_empty_table = ws.expr_ty("{}");
            assert_eq!(
                TypeOps::Narrow.apply(ws.get_db_mut(), &type_string_array_opt, &type_empty_table),
                ws.ty("string[]")
            );
        }
    }
}
