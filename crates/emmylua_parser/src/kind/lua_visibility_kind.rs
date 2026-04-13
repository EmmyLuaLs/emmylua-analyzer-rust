#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum VisibilityKind {
    Public,
    Protected,
    Private,
    Internal,
    Package,
}

impl VisibilityKind {
    pub fn to_visibility_kind(visibility: &str) -> Option<VisibilityKind> {
        match visibility {
            "public" => Some(VisibilityKind::Public),
            "protected" => Some(VisibilityKind::Protected),
            "private" => Some(VisibilityKind::Private),
            "internal" => Some(VisibilityKind::Internal),
            "package" => Some(VisibilityKind::Package),
            _ => None,
        }
    }

    pub fn to_str(&self) -> Option<&'static str> {
        match self {
            VisibilityKind::Public => Some("public"),
            VisibilityKind::Protected => Some("protected"),
            VisibilityKind::Private => Some("private"),
            VisibilityKind::Internal => Some("internal"),
            VisibilityKind::Package => Some("package"),
        }
    }
}
