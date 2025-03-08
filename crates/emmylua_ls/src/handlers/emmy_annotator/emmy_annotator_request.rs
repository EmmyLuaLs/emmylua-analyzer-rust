use lsp_types::{request::Request, Range};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum EmmyAnnotatorRequest {}

impl Request for EmmyAnnotatorRequest {
    type Params = EmmyAnnotatorParams;
    type Result = Option<Vec<EmmyAnnotator>>;
    const METHOD: &'static str = "emmy/annotator";
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub struct EmmyAnnotatorParams {
    pub uri: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub struct EmmyAnnotator {
    #[serde(rename = "type")]
    pub typ: EmmyAnnotatorType,
    pub ranges: Vec<Range>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum EmmyAnnotatorType {
    ReadonlyParam = 0,
    Global = 1,
    ReadOnlyLocal = 2,
    MutLocal = 3,
    MutParam = 4,
}

impl From<EmmyAnnotatorType> for u8 {
    fn from(annotator_type: EmmyAnnotatorType) -> Self {
        annotator_type as u8
    }
}

impl From<u8> for EmmyAnnotatorType {
    fn from(value: u8) -> Self {
        match value {
            0 => EmmyAnnotatorType::ReadonlyParam,
            1 => EmmyAnnotatorType::Global,
            2 => EmmyAnnotatorType::ReadOnlyLocal,
            3 => EmmyAnnotatorType::MutLocal,
            4 => EmmyAnnotatorType::MutParam,
            _ => EmmyAnnotatorType::ReadOnlyLocal,
        }
    }
}
