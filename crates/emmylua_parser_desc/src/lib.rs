use emmylua_parser::{LuaDocDescription, LuaTokenKind};
use rowan::TextRange;

mod md;
mod ref_target;
mod rst;
mod util;

pub use ref_target::*;
use util::sort_result;

#[cfg(test)]
mod testlib;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DescItemKind {
    /// Generic block of documentation.
    Scope,

    /// Cross-reference to a Lua object.
    Ref,

    /// Emphasis.
    Em,

    /// Strong emphasis.
    Strong,

    /// Code markup.
    Code,

    /// Hyperlink.
    Link,

    /// Inline markup, like stars around emphasized text.
    Markup,

    /// Directive name, code-block syntax name, role name,
    /// or some other form of argument.
    Arg,

    /// Line of code in a code block.
    CodeBlock,

    /// Line of code in a code block highlighted by Lua lexer.
    CodeBlockHl(LuaTokenKind),
}

#[derive(Debug, Clone)]
pub struct DescItem {
    pub range: TextRange,
    pub kind: DescItemKind,
}

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
    /// Process a description node and yield found documentation ranges.
    fn parse(&mut self, text: &str, desc: LuaDocDescription) -> Vec<DescItem>;
}

pub fn parse(
    kind: DescParserType,
    text: &str,
    desc: LuaDocDescription,
    cursor_position: Option<usize>,
) -> Vec<DescItem> {
    let mut items = match kind {
        DescParserType::None => Vec::new(),
        DescParserType::Md => md::MdParser::new(cursor_position).parse(text, desc),
        DescParserType::MySt { primary_domain } => {
            md::MdParser::new_myst(primary_domain, cursor_position).parse(text, desc)
        }
        DescParserType::Rst {
            primary_domain,
            default_role,
        } => rst::RstParser::new(primary_domain, default_role, cursor_position).parse(text, desc),
    };

    sort_result(&mut items);

    items
}
