use crate::LuaDocDescription;
use crate::desc_parser::{DescItem, LuaDescParser};

pub struct DummyParser;

impl LuaDescParser for DummyParser {
    fn parse(&mut self, _text: &str, _desc: LuaDocDescription) -> Vec<DescItem> {
        Vec::new()
    }
}
