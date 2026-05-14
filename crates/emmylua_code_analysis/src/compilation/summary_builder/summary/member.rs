use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
};

use internment::ArcIntern;
use rowan::TextSize;
use smol_str::SmolStr;

use super::{SalsaDeclId, SalsaGlobalRootSummary, SalsaSyntaxIdSummary};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaMemberKindSummary {
    Variable,
    Function,
    Method,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaMemberPathRootSummary {
    Env,
    Name(SmolStr),
}

#[derive(Clone)]
pub struct SalsaMemberSegmentsHandle(ArcIntern<Vec<SmolStr>>);

impl SalsaMemberSegmentsHandle {
    pub fn as_slice(&self) -> &[SmolStr] {
        self.0.as_ref().as_slice()
    }

    pub fn with_pushed(&self, segment: SmolStr) -> Self {
        let mut segments = self.as_slice().to_vec();
        segments.push(segment);
        segments.into()
    }
}

impl Default for SalsaMemberSegmentsHandle {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl From<Vec<SmolStr>> for SalsaMemberSegmentsHandle {
    fn from(value: Vec<SmolStr>) -> Self {
        Self(ArcIntern::new(value))
    }
}

impl From<&[SmolStr]> for SalsaMemberSegmentsHandle {
    fn from(value: &[SmolStr]) -> Self {
        value.to_vec().into()
    }
}

impl Deref for SalsaMemberSegmentsHandle {
    type Target = [SmolStr];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl fmt::Debug for SalsaMemberSegmentsHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl PartialEq for SalsaMemberSegmentsHandle {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for SalsaMemberSegmentsHandle {}

impl PartialOrd for SalsaMemberSegmentsHandle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SalsaMemberSegmentsHandle {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl Hash for SalsaMemberSegmentsHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

unsafe impl salsa::Update for SalsaMemberSegmentsHandle {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is provided by salsa and points to initialized storage for `Self`.
        unsafe {
            if *old_pointer == new_value {
                false
            } else {
                old_pointer.write(new_value);
                true
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaMemberPathSummary {
    pub root: SalsaMemberPathRootSummary,
    pub owner_segments: SalsaMemberSegmentsHandle,
    pub member_name: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub enum SalsaMemberRootSummary {
    Global(SalsaGlobalRootSummary),
    LocalDecl { name: SmolStr, decl_id: SalsaDeclId },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct SalsaMemberTargetSummary {
    pub root: SalsaMemberRootSummary,
    pub owner_segments: SalsaMemberSegmentsHandle,
    pub member_name: SmolStr,
}

#[derive(Clone)]
pub struct SalsaMemberTargetId(ArcIntern<SalsaMemberTargetSummary>);

impl SalsaMemberTargetId {
    pub fn as_summary(&self) -> &SalsaMemberTargetSummary {
        self.0.as_ref()
    }
}

impl From<SalsaMemberTargetSummary> for SalsaMemberTargetId {
    fn from(value: SalsaMemberTargetSummary) -> Self {
        Self(ArcIntern::new(value))
    }
}

impl From<&SalsaMemberTargetSummary> for SalsaMemberTargetId {
    fn from(value: &SalsaMemberTargetSummary) -> Self {
        Self(ArcIntern::new(value.clone()))
    }
}

impl From<SalsaMemberTargetId> for SalsaMemberTargetSummary {
    fn from(value: SalsaMemberTargetId) -> Self {
        value.as_summary().clone()
    }
}

impl From<&SalsaMemberTargetId> for SalsaMemberTargetSummary {
    fn from(value: &SalsaMemberTargetId) -> Self {
        value.as_summary().clone()
    }
}

impl Deref for SalsaMemberTargetId {
    type Target = SalsaMemberTargetSummary;

    fn deref(&self) -> &Self::Target {
        self.as_summary()
    }
}

impl fmt::Debug for SalsaMemberTargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_summary().fmt(f)
    }
}

impl PartialEq for SalsaMemberTargetId {
    fn eq(&self, other: &Self) -> bool {
        self.as_summary() == other.as_summary()
    }
}

impl PartialEq<SalsaMemberTargetSummary> for SalsaMemberTargetId {
    fn eq(&self, other: &SalsaMemberTargetSummary) -> bool {
        self.as_summary() == other
    }
}

impl PartialEq<SalsaMemberTargetId> for SalsaMemberTargetSummary {
    fn eq(&self, other: &SalsaMemberTargetId) -> bool {
        self == other.as_summary()
    }
}

impl Eq for SalsaMemberTargetId {}

impl PartialOrd for SalsaMemberTargetId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SalsaMemberTargetId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_summary().cmp(other.as_summary())
    }
}

impl Hash for SalsaMemberTargetId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_summary().hash(state);
    }
}

unsafe impl salsa::Update for SalsaMemberTargetId {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is provided by salsa and points to initialized storage for `Self`.
        unsafe {
            if *old_pointer == new_value {
                false
            } else {
                old_pointer.write(new_value);
                true
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaMemberSummary {
    pub syntax_id: SalsaSyntaxIdSummary,
    pub target: SalsaMemberTargetId,
    pub kind: SalsaMemberKindSummary,
    pub signature_offset: Option<TextSize>,
    pub value_expr_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub value_result_index: usize,
    pub source_call_syntax_id: Option<SalsaSyntaxIdSummary>,
    pub is_method: bool,
}

impl SalsaMemberSummary {
    pub fn value_expr_offset(&self) -> Option<TextSize> {
        self.value_expr_syntax_id
            .map(|syntax_id| syntax_id.start_offset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct SalsaMemberIndexSummary {
    pub members: Vec<SalsaMemberSummary>,
}
