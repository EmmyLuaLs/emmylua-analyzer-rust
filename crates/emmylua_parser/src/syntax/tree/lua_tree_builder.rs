use rowan::{GreenNode, NodeCache};

use crate::{
    kind::{LuaSyntaxKind, LuaTokenKind},
    parser::MarkEvent,
    text::SourceRange,
};

use super::lua_green_builder::LuaGreenNodeBuilder;

#[derive(Debug)]
pub struct LuaTreeBuilder<'a> {
    text: &'a str,
    events: Vec<MarkEvent>,
    green_builder: LuaGreenNodeBuilder<'a>,
}

impl<'a> LuaTreeBuilder<'a> {
    pub fn new(
        text: &'a str,
        events: Vec<MarkEvent>,
        node_cache: Option<&'a mut NodeCache>,
    ) -> Self {
        match node_cache {
            Some(cache) => LuaTreeBuilder {
                text,
                events,
                green_builder: LuaGreenNodeBuilder::with_cache(cache),
            },
            None => LuaTreeBuilder {
                text,
                events,
                green_builder: LuaGreenNodeBuilder::new(),
            },
        }
    }

    pub fn build(&mut self) {
        self.start_node(LuaSyntaxKind::Chunk);
        let mut parents: Vec<LuaSyntaxKind> = Vec::new();
        for i in 0..self.events.len() {
            match std::mem::replace(&mut self.events[i], MarkEvent::none()) {
                MarkEvent::NodeStart {
                    kind: LuaSyntaxKind::None,
                    ..
                }
                | MarkEvent::Trivia => {}
                MarkEvent::NodeStart { kind, parent } => {
                    parents.push(kind);
                    let mut parent_position = parent;
                    while parent_position > 0 {
                        match std::mem::replace(
                            &mut self.events[parent_position],
                            MarkEvent::none(),
                        ) {
                            MarkEvent::NodeStart { kind, parent } => {
                                parents.push(kind);
                                parent_position = parent;
                            }
                            _ => unreachable!(),
                        }
                    }

                    for kind in parents.drain(..).rev() {
                        self.start_node(kind);
                    }
                }
                MarkEvent::NodeEnd => {
                    self.finish_node();
                }
                MarkEvent::EatToken { kind, range } => {
                    self.token(kind, range);
                }
            }
        }

        self.finish_node();
    }

    fn token(&mut self, kind: LuaTokenKind, range: SourceRange) {
        self.green_builder.token(kind, range);
    }

    fn start_node(&mut self, kind: LuaSyntaxKind) {
        self.green_builder.start_node(kind);
    }

    fn finish_node(&mut self) {
        self.green_builder.finish_node();
    }

    pub fn finish(self) -> GreenNode {
        let root = self.green_builder.finish(&self.text);
        root
    }
}
