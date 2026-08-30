#[cfg(test)]
mod test {
    use std::str::FromStr;

    use lsp_types::Uri;

    use crate::{FileId, VirtualWorkspace, WorkspaceFolder};

    fn type_def_visible(ws: &VirtualWorkspace, file_id: FileId, name: &str) -> bool {
        let model = ws
            .analysis
            .semantic_model(file_id)
            .expect("semantic model must exist");
        model.resolve_type_def_in(file_id, name).is_some()
    }

    #[test]
    fn type_decl_visibility_comes_from_type_flags_instead_of_standalone_visibility_tags() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis.add_library_workspace(&WorkspaceFolder::new(
            ws.virtual_url_generator.new_path("lib"),
            true,
        ));
        ws.def_file(
            "lib/types.lua",
            r#"
                ---@namespace Shared

                ---@internal
                ---@class TaggedInternalType
                local TaggedInternalType = {}

                ---@class PlainPublicType
                local PlainPublicType = {}

                ---@class (public) PublicType
                local PublicType = {}

                ---@class (internal) InternalType
                local InternalType = {}

                ---@class (file) PrivateType
                local PrivateType = {}
            "#,
        );
        let library_consumer = ws.def_file("lib/consumer.lua", "local value = 1");
        let consumer = ws.def_file("main.lua", "local value = 1");

        assert!(type_def_visible(
            &ws,
            library_consumer,
            "Shared.TaggedInternalType"
        ));
        assert!(type_def_visible(
            &ws,
            library_consumer,
            "Shared.PlainPublicType"
        ));
        assert!(type_def_visible(&ws, library_consumer, "Shared.PublicType"));
        assert!(type_def_visible(
            &ws,
            library_consumer,
            "Shared.InternalType"
        ));
        assert!(!type_def_visible(
            &ws,
            library_consumer,
            "Shared.PrivateType"
        ));
        assert!(type_def_visible(&ws, consumer, "Shared.TaggedInternalType"));
        assert!(type_def_visible(&ws, consumer, "Shared.PlainPublicType"));
        assert!(type_def_visible(&ws, consumer, "Shared.PublicType"));
        assert!(!type_def_visible(&ws, consumer, "Shared.InternalType"));
        assert!(!type_def_visible(&ws, consumer, "Shared.PrivateType"));
    }

    #[test]
    fn std_workspace_types_are_visible_without_explicit_public() {
        let mut ws = VirtualWorkspace::new();
        let std_root = ws.virtual_url_generator.new_path("std");
        ws.analysis.salsa.add_std_workspace(std_root);
        ws.def_file(
            "std/types.lua",
            r#"
                ---@namespace Shared
                ---@class StdType
                local StdType = {}
            "#,
        );
        let consumer = ws.def_file("main.lua", "local value = 1");

        assert!(type_def_visible(&ws, consumer, "Shared.StdType"));
    }

    #[test]
    fn remote_workspace_types_are_visible_without_explicit_public() {
        let mut ws = VirtualWorkspace::new();
        ws.analysis.update_remote_file_by_uri(
            &Uri::from_str("https://example.com/remote-types.lua").unwrap(),
            Some(
                r#"
                    ---@namespace Shared
                    ---@class RemoteType
                    local RemoteType = {}
                "#
                .to_string(),
            ),
        );
        let consumer = ws.def_file("main.lua", "local value = 1");

        assert!(type_def_visible(&ws, consumer, "Shared.RemoteType"));
    }
}
