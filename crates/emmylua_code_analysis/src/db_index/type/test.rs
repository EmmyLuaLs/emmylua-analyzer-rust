#[cfg(test)]
mod test {
    use rowan::TextRange;

    use crate::db_index::traits::LuaIndex;
    use crate::db_index::r#type::LuaTypeIndex;
    use crate::db_index::{LuaDeclTypeKind, LuaTypeFlag};
    use crate::{FileId, LuaTypeDecl, LuaTypeDeclId, WorkspaceId};

    fn create_type_index() -> LuaTypeIndex {
        LuaTypeIndex::new()
    }

    #[test]
    fn test_namespace() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };
        index.add_file_namespace(file_id, "test".to_string());
        let ns = index.get_file_namespace(&file_id).unwrap();
        assert_eq!(ns, "test");

        let _ = index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "new_type".to_string(),
                LuaDeclTypeKind::Alias,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("test.new_type"),
            ),
        );

        let decl = index.find_type_decl(file_id, "new_type", None);
        assert!(decl.is_some());
        assert_eq!(decl.unwrap().get_name(), "new_type");
        assert!(decl.unwrap().is_alias());
        assert_eq!(decl.unwrap().get_id().get_name(), "test.new_type");

        let file_id2 = FileId { id: 2 };
        let decl2 = index.find_type_decl(file_id2, "test.new_type", None);
        assert!(decl2.is_some());
        assert_eq!(decl2, decl);

        let file_id = FileId { id: 3 };
        let decl3 = index.find_type_decl(file_id, "unknown_type", None);
        assert!(decl3.is_none());
    }

    #[test]
    fn test_using_namespace() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };
        index.add_file_using_namespace(file_id, "test".to_string());
        let ns = index.get_file_using_namespace(&file_id).unwrap();
        assert_eq!(ns, &["test".to_string()]);

        let _ = index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "new_type".to_string(),
                LuaDeclTypeKind::Alias,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("test.new_type"),
            ),
        );

        let decl = index.find_type_decl(file_id, "new_type", None);
        assert!(decl.is_some());
        assert_eq!(decl.unwrap().get_name(), "new_type");
        assert!(decl.unwrap().is_alias());

        let decl2 = index.find_type_decl(file_id, "test.new_type", None);
        assert!(decl2.is_some());
        assert_eq!(decl2, decl);

        let decl3 = index.find_type_decl(file_id, "unknown_type", None);
        assert!(decl3.is_none());
    }

    #[test]
    fn test_type_remove() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };

        let _ = index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "new_type".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("new_type"),
            ),
        );

        let decl = index.find_type_decl(file_id, "new_type", None);
        assert!(decl.is_some());
        index.remove(file_id);
        let decl2 = index.find_type_decl(file_id, "new_type", None);
        assert!(decl2.is_none());

        let _ = index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "new_type".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global(".new_type"),
            ),
        );

        let file_id2 = FileId { id: 2 };
        let _ = index.add_type_decl(
            file_id2,
            LuaTypeDecl::new(
                file_id2,
                TextRange::new(0.into(), 4.into()),
                "new_type".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("new_type"),
            ),
        );

        let decl = index.find_type_decl(file_id, "new_type", None);
        assert!(decl.is_some());
        index.remove(file_id);
        let decl2 = index.find_type_decl(file_id2, "new_type", None);
        assert!(decl2.is_some());
        index.remove(file_id2);
        let decl3 = index.find_type_decl(file_id2, "new_type", None);
        assert!(decl3.is_none());
    }

    #[test]
    fn test_merged_type_decl_stays_indexed_until_last_location_removed() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };
        let file_id2 = FileId { id: 2 };

        index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "Shared".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("Shared"),
            ),
        );
        index.add_type_decl(
            file_id2,
            LuaTypeDecl::new(
                file_id2,
                TextRange::new(4.into(), 8.into()),
                "Shared".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("Shared"),
            ),
        );

        assert!(index.find_type_decl(file_id, "Shared", None).is_some());
        index.remove(file_id);
        assert!(index.find_type_decl(file_id2, "Shared", None).is_some());
        index.remove(file_id2);
        assert!(index.find_type_decl(file_id2, "Shared", None).is_none());
    }

    #[test]
    fn test_type_info() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };

        let _ = index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "new_type".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("test.new_type"),
            ),
        );

        let decl = index
            .find_type_decl(file_id, "test.new_type", None)
            .unwrap();
        assert_eq!(decl.get_name(), "new_type");
        assert!(decl.is_class());
        assert_eq!(decl.get_namespace(), "test".into());
        assert_eq!(decl.get_full_name(), "test.new_type");
    }

    #[test]
    fn test_internal_type_ids_are_workspace_scoped_and_lookup_respects_workspace() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };
        let file_id2 = FileId { id: 2 };
        let outsider = FileId { id: 99 };
        let workspace_id = WorkspaceId { id: 3 };
        let workspace_id2 = WorkspaceId { id: 4 };
        let mut flag = LuaTypeFlag::Partial.into();
        flag |= LuaTypeFlag::Internal;

        index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "Foo".to_string(),
                LuaDeclTypeKind::Class,
                flag,
                LuaTypeDeclId::internal(workspace_id, "Foo"),
            ),
        );
        index.add_type_decl(
            file_id2,
            LuaTypeDecl::new(
                file_id2,
                TextRange::new(0.into(), 4.into()),
                "Foo".to_string(),
                LuaDeclTypeKind::Class,
                flag,
                LuaTypeDeclId::internal(workspace_id2, "Foo"),
            ),
        );

        assert_eq!(index.get_all_types().len(), 2);
        assert_eq!(
            index
                .find_type_decl(file_id, "Foo", Some(workspace_id))
                .unwrap()
                .get_id(),
            LuaTypeDeclId::internal(workspace_id, "Foo")
        );
        assert_eq!(
            index
                .find_type_decl(file_id2, "Foo", Some(workspace_id2))
                .unwrap()
                .get_id(),
            LuaTypeDeclId::internal(workspace_id2, "Foo")
        );
        assert!(
            index
                .find_type_decl(outsider, "Foo", Some(WorkspaceId::MAIN))
                .is_none()
        );
    }

    #[test]
    fn test_find_type_decl_prefers_local_then_internal_then_global() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };
        let outsider = FileId { id: 2 };
        let workspace_id = WorkspaceId { id: 3 };
        let mut internal_flag = LuaTypeFlag::Partial.into();
        internal_flag |= LuaTypeFlag::Internal;

        index.add_type_decl(
            outsider,
            LuaTypeDecl::new(
                outsider,
                TextRange::new(0.into(), 4.into()),
                "Foo".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("Foo"),
            ),
        );
        index.add_type_decl(
            outsider,
            LuaTypeDecl::new(
                outsider,
                TextRange::new(4.into(), 8.into()),
                "Foo".to_string(),
                LuaDeclTypeKind::Class,
                internal_flag,
                LuaTypeDeclId::internal(workspace_id, "Foo"),
            ),
        );
        index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(8.into(), 12.into()),
                "Foo".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::local(file_id, "Foo"),
            ),
        );

        assert_eq!(
            index
                .find_type_decl(file_id, "Foo", Some(workspace_id))
                .unwrap()
                .get_id(),
            LuaTypeDeclId::local(file_id, "Foo")
        );
        assert_eq!(
            index
                .find_type_decl(outsider, "Foo", Some(workspace_id))
                .unwrap()
                .get_id(),
            LuaTypeDeclId::internal(workspace_id, "Foo")
        );
        assert_eq!(
            index
                .find_type_decl(outsider, "Foo", None)
                .unwrap()
                .get_id(),
            LuaTypeDeclId::global("Foo")
        );
    }

    #[test]
    fn test_find_type_decls_filters_internal_types_by_workspace() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };
        let file_id2 = FileId { id: 2 };
        let workspace_id = WorkspaceId { id: 3 };
        let workspace_id2 = WorkspaceId { id: 4 };
        let mut internal_flag = LuaTypeFlag::Partial.into();
        internal_flag |= LuaTypeFlag::Internal;

        index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "PublicOnly".to_string(),
                LuaDeclTypeKind::Class,
                LuaTypeFlag::Partial.into(),
                LuaTypeDeclId::global("PublicOnly"),
            ),
        );
        index.add_type_decl(
            file_id,
            LuaTypeDecl::new(
                file_id,
                TextRange::new(0.into(), 4.into()),
                "VisibleInternal".to_string(),
                LuaDeclTypeKind::Class,
                internal_flag,
                LuaTypeDeclId::internal(workspace_id, "VisibleInternal"),
            ),
        );
        index.add_type_decl(
            file_id2,
            LuaTypeDecl::new(
                file_id2,
                TextRange::new(0.into(), 4.into()),
                "HiddenInternal".to_string(),
                LuaDeclTypeKind::Class,
                internal_flag,
                LuaTypeDeclId::internal(workspace_id2, "HiddenInternal"),
            ),
        );

        let visible = index.find_type_decls(file_id, "", Some(workspace_id));
        assert!(visible.contains_key("PublicOnly"));
        assert!(visible.contains_key("VisibleInternal"));
        assert!(!visible.contains_key("HiddenInternal"));

        let visible2 = index.find_type_decls(file_id2, "", Some(workspace_id2));
        assert!(visible2.contains_key("PublicOnly"));
        assert!(visible2.contains_key("HiddenInternal"));
        assert!(!visible2.contains_key("VisibleInternal"));
    }

    #[test]
    fn test_internal_type_decl_id_serialization_roundtrip() {
        let decl_id = LuaTypeDeclId::internal(WorkspaceId { id: 42 }, "Foo.Bar");
        let encoded = serde_json::to_string(&decl_id).unwrap();
        let decoded: LuaTypeDeclId = serde_json::from_str(&encoded).unwrap();

        assert_eq!(encoded, "\"ws:42|Foo.Bar\"");
        assert_eq!(decoded, decl_id);
    }
}
