#[cfg(test)]
mod test {
    // use crate::{DiagnosticCode, VirtualWorkspace};

    // #[test]
    // fn test() {
    //     let mut ws = VirtualWorkspace::new();
    //     assert!(!ws.check_code_for(
    //         DiagnosticCode::UndefinedField,
    //         r#"
    //             ---@class diagnostic.test3
    //             ---@field private a number

    //             ---@type diagnostic.test3
    //             local test = {}

    //             local b = test.b
    //         "#
    //     ));

    //     assert!(!ws.check_code_for(
    //         DiagnosticCode::UndefinedField,
    //         r#"
    //             ---@class diagnostic.test3
    //             ---@field private a number
    //             local Test3 = {}

    //             local b = Test3.b
    //         "#
    //     ));
    // }

    // #[test]
    // fn test_enum() {
    //     let mut ws = VirtualWorkspace::new();
    //     assert!(!ws.check_code_for(
    //         DiagnosticCode::UndefinedField,
    //         r#"
    //             ---@enum diagnostic.enum
    //             local Enum = {
    //                 A = 1,
    //             }

    //             local enum_b = Enum["B"]
    //         "#
    //     ));
    // }
}
