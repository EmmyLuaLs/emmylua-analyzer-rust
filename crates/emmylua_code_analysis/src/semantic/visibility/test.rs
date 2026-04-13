#[cfg(test)]
mod test {
    use std::str::FromStr;

    use lsp_types::Uri;

    use crate::{DbIndex, FileId, LuaTypeDecl, VirtualWorkspace, WorkspaceFolder, WorkspaceId};

    fn find_visible_type_decl<'a>(
        db: &'a DbIndex,
        file_id: FileId,
        name: &str,
    ) -> Option<&'a LuaTypeDecl> {
        db.get_type_index()
            .find_type_decl(file_id, name, db.resolve_workspace_id(file_id))
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

                ---@class (private) PrivateType
                local PrivateType = {}
            "#,
        );
        let library_consumer = ws.def_file("lib/consumer.lua", "local value = 1");
        let consumer = ws.def_file("main.lua", "local value = 1");

        let db = ws.analysis.compilation.get_db();
        assert!(
            find_visible_type_decl(db, library_consumer, "Shared.TaggedInternalType").is_some()
        );
        assert!(find_visible_type_decl(db, library_consumer, "Shared.PlainPublicType").is_some());
        assert!(find_visible_type_decl(db, library_consumer, "Shared.PublicType").is_some());
        assert!(find_visible_type_decl(db, library_consumer, "Shared.InternalType").is_some());
        assert!(find_visible_type_decl(db, library_consumer, "Shared.PrivateType").is_none());
        assert!(find_visible_type_decl(db, consumer, "Shared.TaggedInternalType").is_some());
        assert!(find_visible_type_decl(db, consumer, "Shared.PlainPublicType").is_some());
        assert!(find_visible_type_decl(db, consumer, "Shared.PublicType").is_some());
        assert!(find_visible_type_decl(db, consumer, "Shared.InternalType").is_none());
        assert!(find_visible_type_decl(db, consumer, "Shared.PrivateType").is_none());
    }

    #[test]
    fn std_workspace_types_are_visible_without_explicit_public() {
        let mut ws = VirtualWorkspace::new();
        let std_root = ws.virtual_url_generator.new_path("std");
        ws.analysis
            .compilation
            .get_db_mut()
            .get_module_index_mut()
            .add_workspace_root(std_root, WorkspaceId::STD);
        ws.def_file(
            "std/types.lua",
            r#"
                ---@namespace Shared
                ---@class StdType
                local StdType = {}
            "#,
        );
        let consumer = ws.def_file("main.lua", "local value = 1");

        let db = ws.analysis.compilation.get_db();
        assert!(find_visible_type_decl(db, consumer, "Shared.StdType").is_some());
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

        let db = ws.analysis.compilation.get_db();
        assert!(find_visible_type_decl(db, consumer, "Shared.RemoteType").is_some());
    }
}
