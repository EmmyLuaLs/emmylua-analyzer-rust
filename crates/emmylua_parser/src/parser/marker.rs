use crate::{
    kind::{LuaSyntaxKind, LuaTokenKind},
    text::SourceRange,
};

#[derive(Debug, Clone)]
pub enum MarkEvent {
    NodeStart {
        kind: LuaSyntaxKind,
        parent: usize,
    },
    EatToken {
        kind: LuaTokenKind,
        range: SourceRange,
    },
    NodeEnd,
    Trivia,
}

impl MarkEvent {
    pub fn none() -> Self {
        MarkEvent::NodeStart {
            kind: LuaSyntaxKind::None,
            parent: 0,
        }
    }
}

pub(crate) trait MarkerEventContainer {
    fn get_mark_level(&self) -> usize;

    fn incr_mark_level(&mut self);

    fn decr_mark_level(&mut self);

    fn get_events(&mut self) -> &mut Vec<MarkEvent>;

    fn mark(&mut self, kind: LuaSyntaxKind) -> Marker {
        let position = self.get_events().len();
        self.get_events()
            .push(MarkEvent::NodeStart { kind, parent: 0 });
        self.incr_mark_level();
        Marker::new(position)
    }

    fn push_node_end(&mut self) {
        self.decr_mark_level();
        self.get_events().push(MarkEvent::NodeEnd);
    }
}

pub(crate) struct Marker {
    pub position: usize,
}

impl Marker {
    pub fn new(position: usize) -> Self {
        Marker { position }
    }

    pub fn set_kind<P: MarkerEventContainer>(&mut self, p: &mut P, kind: LuaSyntaxKind) {
        match &mut p.get_events()[self.position] {
            MarkEvent::NodeStart { kind: k, .. } => *k = kind,
            _ => unreachable!(),
        }
    }

    pub fn complete<P: MarkerEventContainer>(self, p: &mut P) -> CompleteMarker {
        let kind = match p.get_events()[self.position] {
            MarkEvent::NodeStart { kind: k, .. } => k,
            _ => unreachable!(),
        };

        let finish = p.get_events().len();
        // empty node should be removed
        if finish == self.position + 1 {
            match &mut p.get_events()[self.position] {
                MarkEvent::NodeStart { kind, .. } => *kind = LuaSyntaxKind::None,
                _ => unreachable!(),
            }
            return CompleteMarker {
                start: 0,
                finish: 0,
                kind: LuaSyntaxKind::None,
            };
        }
        p.push_node_end();
        CompleteMarker {
            start: self.position,
            finish,
            kind,
        }
    }

    #[allow(unused)]
    pub fn undo<P: MarkerEventContainer>(self, p: &mut P) {
        match &mut p.get_events()[self.position] {
            MarkEvent::NodeStart { kind, .. } => {
                *kind = LuaSyntaxKind::None;
            }
            _ => unreachable!(),
        }
    }
}

pub(crate) struct CompleteMarker {
    #[allow(unused)]
    pub start: usize,
    #[allow(unused)]
    pub finish: usize,
    pub kind: LuaSyntaxKind,
}

impl CompleteMarker {
    pub fn precede<P: MarkerEventContainer>(&self, p: &mut P, kind: LuaSyntaxKind) -> Marker {
        let m = p.mark(kind);
        match &mut p.get_events()[self.start] {
            MarkEvent::NodeStart { parent, .. } => *parent = m.position,
            _ => unreachable!(),
        }
        p.get_events().push(MarkEvent::Trivia);
        m
    }

    pub fn empty() -> Self {
        CompleteMarker {
            start: 0,
            finish: 0,
            kind: LuaSyntaxKind::None,
        }
    }
}
