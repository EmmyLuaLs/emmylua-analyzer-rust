//! Module export visibility (mirrors legacy `ModuleVisibility`: Public / Internal / Hide).

use emmylua_parser::VisibilityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ModuleVisibility {
    #[default]
    Public,
    Internal,
    Hide,
}

impl ModuleVisibility {
    pub fn from_visibility_kind(visibility: VisibilityKind) -> Option<Self> {
        match visibility {
            VisibilityKind::Public => Some(Self::Public),
            VisibilityKind::Internal => Some(Self::Internal),
            _ => None,
        }
    }

    pub fn merge(self, visibility: Self) -> Self {
        match (self, visibility) {
            (Self::Hide, _) | (_, Self::Hide) => Self::Hide,
            (Self::Public, next) => next,
            (current, Self::Public) => current,
            (_, next) => next,
        }
    }

    pub fn is_visible_outside(&self) -> bool {
        matches!(self, ModuleVisibility::Public)
    }

    pub fn is_hidden(&self) -> bool {
        matches!(self, ModuleVisibility::Hide)
    }
}
