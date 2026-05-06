use std::cmp;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub struct FileId {
    pub id: u32,
}

impl Serialize for FileId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.id)
    }
}

impl<'de> Deserialize<'de> for FileId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let id = u32::deserialize(deserializer)?;
        Ok(FileId { id })
    }
}

impl FileId {
    pub fn new(id: u32) -> Self {
        FileId { id }
    }

    pub const VIRTUAL: FileId = FileId { id: u32::MAX };
}

impl From<u32> for FileId {
    fn from(id: u32) -> Self {
        FileId { id }
    }
}

impl PartialOrd for FileId {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FileId {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct InFiled<N> {
    pub file_id: FileId,
    pub value: N,
}

impl<N> InFiled<N> {
    pub fn new(file_id: FileId, value: N) -> Self {
        InFiled { file_id, value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_file_id_serialization() {
        let file_id = FileId { id: 42 };
        let serialized = serde_json::to_string(&file_id).unwrap();
        // u32 is serialized as a number, so the JSON representation is "42"
        assert_eq!(serialized, "42");
        let deserialized: FileId = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, file_id);
    }

    #[test]
    fn test_file_id_new_and_virtual() {
        let new_file_id = FileId::new(0);
        assert_eq!(new_file_id.id, 0);

        let virtual_id = FileId::VIRTUAL;
        assert_eq!(virtual_id.id, u32::MAX);
    }

    #[test]
    fn test_infiled_new() {
        let file_id = FileId { id: 10 };
        let infiled = InFiled::new(file_id, "test_value");
        assert_eq!(infiled.file_id, file_id);
        assert_eq!(infiled.value, "test_value");
    }

    #[test]
    fn test_file_id_deserialization_error() {
        // Provide an invalid JSON value for FileId to trigger an error.
        let json_invalid = "[42]";
        let result: Result<FileId, _> = serde_json::from_str(json_invalid);
        assert!(result.is_err());
    }
}
