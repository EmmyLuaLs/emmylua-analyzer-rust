#[cfg(test)]
mod test {
    use rowan::TextRange;

    use crate::db_index::r#type::LuaTypeIndex;
    use crate::db_index::traits::LuaIndex;
    use crate::db_index::{LuaDeclTypeKind, LuaTypeAttribute};
    use crate::FileId;

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
            TextRange::new(0.into(), 4.into()),
            "new_type".to_string(),
            LuaDeclTypeKind::Alias,
            Some(LuaTypeAttribute::Partial.into()),
        );

        let decl = index.find_type_decl(file_id, "new_type");
        assert!(decl.is_some());
        assert_eq!(decl.unwrap().get_name(), "new_type");
        assert_eq!(decl.unwrap().get_kind(), LuaDeclTypeKind::Alias);
        assert_eq!(
            decl.unwrap().get_attrib(),
            Some(LuaTypeAttribute::Partial.into())
        );
        assert_eq!(decl.unwrap().get_id().get_name(), "test.new_type");

        let file_id2 = FileId { id: 2 };
        let decl2 = index.find_type_decl(file_id2, "test.new_type");
        assert!(decl2.is_some());
        assert_eq!(decl2, decl);

        let file_id = FileId { id: 3 };
        let decl3 = index.find_type_decl(file_id, "unknown_type");
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
            TextRange::new(0.into(), 4.into()),
            "test.new_type".to_string(),
            LuaDeclTypeKind::Alias,
            Some(LuaTypeAttribute::Partial.into()),
        );

        let decl = index.find_type_decl(file_id, "new_type");
        assert!(decl.is_some());
        assert_eq!(decl.unwrap().get_name(), "new_type");
        assert_eq!(decl.unwrap().get_kind(), LuaDeclTypeKind::Alias);
        assert_eq!(
            decl.unwrap().get_attrib(),
            Some(LuaTypeAttribute::Partial.into())
        );

        let decl2 = index.find_type_decl(file_id, "test.new_type");
        assert!(decl2.is_some());
        assert_eq!(decl2, decl);

        let decl3 = index.find_type_decl(file_id, "unknown_type");
        assert!(decl3.is_none());
    }

    #[test]
    fn test_type_remove() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };

        let _ = index.add_type_decl(
            file_id,
            TextRange::new(0.into(), 4.into()),
            "new_type".to_string(),
            LuaDeclTypeKind::Class,
            None,
        );

        let decl = index.find_type_decl(file_id, "new_type");
        assert!(decl.is_some());
        index.remove(file_id);
        let decl2 = index.find_type_decl(file_id, "new_type");
        assert!(decl2.is_none());

        let _ = index.add_type_decl(
            file_id,
            TextRange::new(0.into(), 4.into()),
            "new_type".to_string(),
            LuaDeclTypeKind::Class,
            Some(LuaTypeAttribute::Partial.into()),
        );

        let file_id2 = FileId { id: 2 };
        let _ = index.add_type_decl(
            file_id2,
            TextRange::new(0.into(), 4.into()),
            "new_type".to_string(),
            LuaDeclTypeKind::Class,
            Some(LuaTypeAttribute::Partial.into()),
        );

        let decl = index.find_type_decl(file_id, "new_type");
        assert!(decl.is_some());
        index.remove(file_id);
        let decl2 = index.find_type_decl(file_id2, "new_type");
        assert!(decl2.is_some());
        index.remove(file_id2);
        let decl3 = index.find_type_decl(file_id2, "new_type");
        assert!(decl3.is_none());
    }

    #[test]
    fn test_type_info() {
        let mut index = create_type_index();
        let file_id = FileId { id: 1 };

        let _ = index.add_type_decl(
            file_id,
            TextRange::new(0.into(), 4.into()),
            "test.new_type".to_string(),
            LuaDeclTypeKind::Class,
            Some(LuaTypeAttribute::Partial.into()),
        );

        let decl = index.find_type_decl(file_id, "test.new_type").unwrap();
        assert_eq!(decl.get_name(), "new_type");
        assert_eq!(decl.get_kind(), LuaDeclTypeKind::Class);
        assert_eq!(decl.get_attrib(), Some(LuaTypeAttribute::Partial.into()));
        assert_eq!(decl.get_namespace(), "test".into());
        assert_eq!(decl.get_full_name(), "test.new_type");
    }
}
