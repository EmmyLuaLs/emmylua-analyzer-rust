use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceId {
    Std,
    Main,
    Library(u32),
}

impl WorkspaceId {
    pub fn is_library(&self) -> bool {
        matches!(self, WorkspaceId::Library(_))
    }

    pub fn is_main(&self) -> bool {
        self == &WorkspaceId::Main
    }

    pub fn is_std(&self) -> bool {
        self == &WorkspaceId::Std
    }

    pub fn unique_id(&self) -> u32 {
        match self {
            WorkspaceId::Std => 0,
            WorkspaceId::Main => 1,
            WorkspaceId::Library(id) => *id + 2,
        }
    }

    pub fn from_unique_id(id: u32) -> Self {
        match id {
            0 => WorkspaceId::Std,
            1 => WorkspaceId::Main,
            n => WorkspaceId::Library(n - 2),
        }
    }
}

impl PartialOrd for WorkspaceId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WorkspaceId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (WorkspaceId::Std, WorkspaceId::Std) => std::cmp::Ordering::Equal,
            (WorkspaceId::Std, _) => std::cmp::Ordering::Less,
            (_, WorkspaceId::Std) => std::cmp::Ordering::Greater,
            (WorkspaceId::Main, WorkspaceId::Main) => std::cmp::Ordering::Equal,
            (WorkspaceId::Main, _) => std::cmp::Ordering::Less,
            (_, WorkspaceId::Main) => std::cmp::Ordering::Greater,
            (WorkspaceId::Library(a), WorkspaceId::Library(b)) => a.cmp(b),
        }
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkspaceId::Std => write!(f, "std"),
            WorkspaceId::Main => write!(f, "main"),
            WorkspaceId::Library(id) => write!(f, "lib{}", id),
        }
    }
}
