use crate::parser::MarkEvent;

mod md;
mod rst;
mod util;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum DescParserType {
    None,
    Md,
    MySt {
        primary_domain: Option<String>,
    },
    Rst {
        primary_domain: Option<String>,
        default_role: Option<String>,
    },
}

impl Default for DescParserType {
    fn default() -> Self {
        DescParserType::None
    }
}

/// Parses markup in comments.
pub trait LuaDescParser {
    /// Process comment lines and yield a syntax tree with parsed comments.
    ///
    /// This function expects to see events as yielded by doc parser. Namely,
    /// for each line there should be a single `TkNormalStart` (or similar),
    /// an optional `TkDocDetail`, and a `TkEndOfLine`.
    fn parse(&mut self, text: &str, tokens: &[MarkEvent]) -> Vec<MarkEvent>;
}

pub fn make_desc_parser(kind: DescParserType) -> Option<Box<dyn LuaDescParser>> {
    match kind {
        DescParserType::None => None,
        DescParserType::Md => Some(Box::new(md::MdParser::new())),
        DescParserType::MySt { primary_domain } => {
            Some(Box::new(md::MdParser::new_myst(primary_domain)))
        }
        DescParserType::Rst {
            primary_domain,
            default_role,
        } => Some(Box::new(rst::RstParser::new(primary_domain, default_role))),
    }
}
