#[cfg(test)]
mod tests {

    use crate::{DiagnosticCode, VirtualWorkspace};

    #[test]
    fn test_290() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
                ---@param a string
                local function foo(_, a)
                    _ = a
                end
            "#
        ));
    }

    #[test]
    fn test_return() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local c = function(x, y)
                return x + y
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local function do_add(x, y)
                return x + y
            end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            local function noop()
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@param p number
            local function FLPR3(p, e)
                return 0
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@param p number
            local function FLPR3(p)
                return 0
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            --- function without param signature
            local function FLPR3(p)
                return 0
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"

                ---@class Test
                local Test = {}

                ---@param test Test
                function Test:add(test, c)
                end
            "#
        ));
    }

    #[test]
    fn test_return_overload() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@return_overload true, integer
            ---@return_overload false, string
            local function f()
                return true, 1
            end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@return_overload true, integer
            ---@return_overload false, string
            local function f()
                return true, 1, "extra"
            end
            "#
        ));
    }

    #[test]
    fn test_variadic_return_overload_does_not_trigger_incomplete_signature_doc() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
            ---@return_overload true, integer...
            ---@return_overload false, string
            local function f()
                return true, 1, 2, 3, 4
            end
            "#
        ));
    }

    #[test]
    fn test_global() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                function FLPR1()
                end
            "#
        ));

        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR1(a)
                end
            "#
        ));
        assert!(!ws.has_no_diagnostic(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR1()
                    return 1
                end
            "#
        ));

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::MissingGlobalDoc,
            r#"
                ---
                function FLPR2()
                end
            "#
        ));
    }

    #[test]
    fn test_incomplete_signature_doc_no_param_no_return() {
        let mut ws = VirtualWorkspace::new();
        ws.enable_full_diagnostic();

        assert!(ws.has_no_diagnostic(
            DiagnosticCode::IncompleteSignatureDoc,
            r#"
                local function a()
                end
            "#
        ));
    }

    /// A closure stored at a nested path (`L.out.to = function(line)`) takes its params from the
    /// field it is stored in, as one stored through a one-level path does.
    #[test]
    fn test_nested_member_closure_params_from_field() {
        fn param_type(ws: &mut VirtualWorkspace, file: &str, src: &str) -> String {
            let file_id = ws.def_file(file, src);
            let model = ws.analysis.semantic_model(file_id);
            let facts = model.file_facts().expect("facts");
            let decl = facts
                .decls
                .iter()
                .find(|decl| decl.name == "line")
                .expect("param decl")
                .id
                .clone();
            let ty = model.type_of_decl(&decl).unwrap_or(crate::LuaType::Unknown);
            ws.humanize_type(ty)
        }
        let mut ws = VirtualWorkspace::new();
        let decl =
            "local L = { out = { to = nil } } ---@type { out: { to: fun(line: string)? } }\n";
        let assigned = format!("{decl}L.out.to = function(line) return line end\n");
        assert_eq!(param_type(&mut ws, "a.lua", &assigned), "string");
        let declared = format!("{decl}function L.out.to(line) return line end\n");
        assert_eq!(param_type(&mut ws, "f.lua", &declared), "string");
        // Across files, the shape luagnd's `gradle.out.to = function(line)` has.
        ws.def_file(
            "reg.lua",
            "local out = { to = nil } ---@type { to: fun(line: string)? }\nlocal M = { out = out }\nreturn M\n",
        );
        let required = "local reg = require 'reg'\nreg.out.to = function(line) return line end\n";
        assert_eq!(param_type(&mut ws, "r.lua", required), "string");
    }

    /// An undocumented function that forwards a call into a `require`d module returns what that
    /// call returns, seen from any file: `delay` forwards `sleep`, which forwards `suspend`.
    #[test]
    fn test_cross_file_forwarded_variadic_return() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "susp.lua",
            "local S = {}\n---@return integer ...\nfunction S.suspend() return 1 end\nreturn S\n",
        );
        ws.def_file(
            "slp.lua",
            "local susp = require 'susp'\nlocal V = {}\nfunction V.sleep() return susp.suspend() end\nreturn V\n",
        );
        ws.def_file(
            "dly.lua",
            "local slp = require 'slp'\nlocal D = {}\nfunction D.delay() return slp.sleep() end\nreturn D\n",
        );
        let ty = ws.expr_ty("require('dly').delay()");
        assert_eq!(ws.humanize_type(ty), "integer");
    }

    /// The same forward, typed: a caller in a third file sees the forwarded `integer`.
    #[test]
    fn test_cross_file_forwarded_call_return_type() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "one.lua",
            "local O = {}\n---@return integer\nfunction O.get() return 1 end\nreturn O\n",
        );
        ws.def_file(
            "fwd.lua",
            "local one = require 'one'\nlocal F = {}\nfunction F.get() return one.get() end\nreturn F\n",
        );
        let ty = ws.expr_ty("require('fwd').get()");
        assert_eq!(ws.humanize_type(ty), "integer");
    }

    /// A forward through a value of a class from another file: `o:get()` is found on `A`, the
    /// prefix's type, not on the local `o`.
    #[test]
    fn test_cross_file_forwarded_call_on_class_value() {
        let mut ws = VirtualWorkspace::new();
        ws.def_file(
            "a.lua",
            "---@class A\nlocal A = {}\n---@return A\nfunction A.new() return setmetatable({}, { __index = A }) end\n---@return integer\nfunction A:get() return 1 end\nreturn A\n",
        );
        ws.def_file(
            "b.lua",
            "local A = require 'a'\nlocal B = {}\nfunction B.get() local o = A.new() return o:get() end\nreturn B\n",
        );
        let ty = ws.expr_ty("require('b').get()");
        assert_eq!(ws.humanize_type(ty), "integer");
    }

    /// A generic factory, in another file or the same one: the call's shell does not bind the
    /// callee's `V`, so a local holding the result reads as `Box<unknown>`, not `Box<V>` with `V`
    /// a stray name.
    #[test]
    fn test_generic_factory_does_not_leak_its_generic() {
        use crate::semantic_db::types::TypeCandidate;
        const BOX: &str = r#"
            ---@class Box<T>
            ---@field v T
            local Box = {}
            ---@generic V
            ---@param v V
            ---@return Box<V>
            function Box.from(v) return setmetatable({ v = v }, { __index = Box }) end
        "#;
        let mut ws = VirtualWorkspace::new();
        ws.def_file("box.lua", &format!("{BOX}\nreturn Box\n"));
        let cross_file = "local Box = require 'box'\nlocal b = Box.from(1)\n".to_string();
        let same_file = format!("{BOX}\nlocal b = Box.from(1)\n");
        for (file, source) in [("use.lua", cross_file), ("same.lua", same_file)] {
            let file_id = ws.def_file(file, &source);
            let model = ws.analysis.semantic_model(file_id);
            let decl = model
                .file_facts()
                .expect("facts")
                .decls
                .iter()
                .find(|d| d.name == "b")
                .expect("decl b")
                .id
                .clone();
            // The shell is what a caller falls back to when the VM cannot type the call (luagnd's
            // `Seq.from`), so check the shell itself.
            let shell = model.analysis().decl_type(file_id, decl).expect("shell");
            assert!(
                matches!(
                    shell.candidates.as_slice(),
                    [TypeCandidate::GenericInstance(instance)]
                        if instance.name == "Box"
                            && instance.args.len() == 1
                            && instance.args[0].is_unknown()
                ),
                "{file}: expected Box<unknown>, got {shell:?}"
            );
        }
    }
}
