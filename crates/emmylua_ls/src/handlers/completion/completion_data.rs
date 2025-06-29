use emmylua_code_analysis::{FileId, LuaSemanticDeclId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::completion_builder::CompletionBuilder;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionData {
    pub field_id: FileId,
    pub typ: CompletionDataType,
    /// Total count of function overloads
    pub overload_count: Option<usize>,
}

#[allow(unused)]
impl CompletionData {
    pub fn from_property_owner_id(
        builder: &CompletionBuilder,
        id: LuaSemanticDeclId,
        overload_count: Option<usize>,
    ) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            typ: CompletionDataType::PropertyOwnerId(id),
            overload_count,
        };
        Some(serde_json::to_value(data).unwrap())
    }

    pub fn from_overload(
        builder: &CompletionBuilder,
        id: LuaSemanticDeclId,
        index: usize,
        overload_count: Option<usize>,
    ) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            typ: CompletionDataType::Overload((id, index)),
            overload_count,
        };
        Some(serde_json::to_value(data).unwrap())
    }

    pub fn from_module(builder: &CompletionBuilder, module: String) -> Option<Value> {
        let data = Self {
            field_id: builder.semantic_model.get_file_id(),
            typ: CompletionDataType::Module(module),
            overload_count: None,
        };
        Some(serde_json::to_value(data).unwrap())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompletionDataType {
    PropertyOwnerId(LuaSemanticDeclId),
    Module(String),
    Overload((LuaSemanticDeclId, usize)),
}

// // Custom serialization implementation
// impl Serialize for CompletionData {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         // Compact format: "field_id|type_flag:type_data|overload_count"
//         // type_flag: P=PropertyOwnerId, M=Module, O=Overload
//         let type_part = match &self.typ {
//             CompletionDataType::PropertyOwnerId(id) => {
//                 format!("P:{}", serde_json::to_string(id).map_err(serde::ser::Error::custom)?)
//             },
//             CompletionDataType::Module(module) => {
//                 format!("M:{}", module)
//             },
//             CompletionDataType::Overload((id, index)) => {
//                 format!("O:{}#{}",
//                     serde_json::to_string(id).map_err(serde::ser::Error::custom)?,
//                     index
//                 )
//             },
//         };

//         let overload_part = match self.overload_count {
//             Some(count) => format!("|{}", count),
//             None => String::new(),
//         };

//         let compact = format!("{}|{}{}", self.field_id.id, type_part, overload_part);
//         serializer.serialize_str(&compact)
//     }
// }

// impl<'de> Deserialize<'de> for CompletionData {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         struct CompletionDataVisitor;

//         impl<'de> Visitor<'de> for CompletionDataVisitor {
//             type Value = CompletionData;

//             fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//                 formatter.write_str("a string with format 'field_id|type_flag:type_data|overload_count'")
//             }

//             fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
//             where
//                 E: de::Error,
//             {
//                 let parts: Vec<&str> = value.split('|').collect();
//                 if parts.len() < 2 || parts.len() > 3 {
//                     return Err(E::custom("expected format 'field_id|type_flag:type_data|overload_count'"));
//                 }

//                 // Parse field_id
//                 let field_id = FileId::new(
//                     parts[0]
//                         .parse()
//                         .map_err(|e| E::custom(format!("invalid field_id: {}", e)))?
//                 );

//                 // Parse type
//                 let type_part = parts[1];
//                 let typ = if let Some(colon_pos) = type_part.find(':') {
//                     let type_flag = &type_part[..colon_pos];
//                     let type_data = &type_part[colon_pos + 1..];

//                     match type_flag {
//                         "P" => {
//                             let id: LuaSemanticDeclId = serde_json::from_str(type_data)
//                                 .map_err(|e| E::custom(format!("invalid PropertyOwnerId: {}", e)))?;
//                             CompletionDataType::PropertyOwnerId(id)
//                         },
//                         "M" => {
//                             CompletionDataType::Module(type_data.to_string())
//                         },
//                         "O" => {
//                             if let Some(hash_pos) = type_data.find('#') {
//                                 let id_part = &type_data[..hash_pos];
//                                 let index_part = &type_data[hash_pos + 1..];

//                                 let id: LuaSemanticDeclId = serde_json::from_str(id_part)
//                                     .map_err(|e| E::custom(format!("invalid Overload id: {}", e)))?;
//                                 let index: usize = index_part
//                                     .parse()
//                                     .map_err(|e| E::custom(format!("invalid Overload index: {}", e)))?;

//                                 CompletionDataType::Overload((id, index))
//                             } else {
//                                 return Err(E::custom("expected '#' separator in Overload type"));
//                             }
//                         },
//                         _ => {
//                             return Err(E::custom(format!("unknown type flag: {}", type_flag)));
//                         }
//                     }
//                 } else {
//                     return Err(E::custom("expected ':' separator in type part"));
//                 };

//                 // Parse overload_count
//                 let overload_count = if parts.len() == 3 {
//                     if parts[2].is_empty() {
//                         None
//                     } else {
//                         Some(
//                             parts[2]
//                                 .parse()
//                                 .map_err(|e| E::custom(format!("invalid overload count: {}", e)))?
//                         )
//                     }
//                 } else {
//                     None
//                 };

//                 Ok(CompletionData {
//                     field_id,
//                     typ,
//                     overload_count,
//                 })
//             }
//         }

//         deserializer.deserialize_str(CompletionDataVisitor)
//     }
// }

// #[cfg(test)]
// mod tests {
//     use emmylua_code_analysis::{FileId, LuaSemanticDeclId, LuaTypeDeclId};

//     use super::{CompletionData, CompletionDataType};

//     #[test]
//     fn test_compact_serialization() {
//         let type_id = LuaTypeDeclId::new("hello world");
//         let data = CompletionData {
//             field_id: FileId::new(1),
//             typ: CompletionDataType::PropertyOwnerId(LuaSemanticDeclId::TypeDecl(type_id)),
//             overload_count: Some(3),
//         };

//         // Test serialization
//         let json = serde_json::to_string(&data).unwrap();
//         println!("Compact serialized: {}", json);

//         // Test deserialization
//         let deserialized: CompletionData = serde_json::from_str(&json).unwrap();
//         assert_eq!(data, deserialized);

//         // Verify the compactness of serialization format
//         assert!(json.len() < 200); // Should be more compact than default JSON serialization
//     }

//     #[test]
//     fn test_module_serialization() {
//         let data = CompletionData {
//             field_id: FileId::new(42),
//             typ: CompletionDataType::Module("socket.core".to_string()),
//             overload_count: None,
//         };

//         let json = serde_json::to_string(&data).unwrap();
//         println!("Module serialized: {}", json);

//         let deserialized: CompletionData = serde_json::from_str(&json).unwrap();
//         assert_eq!(data, deserialized);
//     }

//     #[test]
//     fn test_overload_serialization() {
//         let type_id = LuaTypeDeclId::new("test_function");
//         let data = CompletionData {
//             field_id: FileId::new(10),
//             typ: CompletionDataType::Overload((LuaSemanticDeclId::TypeDecl(type_id), 2)),
//             overload_count: Some(5),
//         };

//         let json = serde_json::to_string(&data).unwrap();
//         println!("Overload serialized: {}", json);

//         let deserialized: CompletionData = serde_json::from_str(&json).unwrap();
//         assert_eq!(data, deserialized);
//     }

//     #[test]
//     fn test_size_comparison() {
//         let type_id = LuaTypeDeclId::new("comparison_test");
//         let data = CompletionData {
//             field_id: FileId::new(999),
//             typ: CompletionDataType::PropertyOwnerId(LuaSemanticDeclId::TypeDecl(type_id.clone())),
//             overload_count: Some(10),
//         };

//         // Our compact serialization
//         let compact_json = serde_json::to_string(&data).unwrap();

//         // Create a struct using default serialization to compare sizes
//         #[derive(serde::Serialize)]
//         struct DefaultSerialized {
//             field_id: u32,
//             typ: CompletionDataType,
//             overload_count: Option<usize>,
//         }

//         let default_data = DefaultSerialized {
//             field_id: data.field_id.id,
//             typ: data.typ.clone(),
//             overload_count: data.overload_count,
//         };

//         let default_json = serde_json::to_string(&default_data).unwrap();

//         println!("Compact size: {} bytes", compact_json.len());
//         println!("Default size: {} bytes", default_json.len());

//         // Compact serialization should be smaller
//         assert!(compact_json.len() <= default_json.len());
//     }
// }
