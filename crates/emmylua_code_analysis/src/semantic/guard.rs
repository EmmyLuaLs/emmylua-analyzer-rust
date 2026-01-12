use std::{cell::RefCell, collections::HashSet, rc::Rc};

use crate::{InferFailReason, LuaTypeDeclId};

pub type InferGuardRef = Rc<InferGuard>;

/// Guard to prevent infinite recursion with optimized lazy allocation
///
/// This guard uses a lazy allocation strategy:
/// - Fork is zero-cost (no HashSet allocation)
/// - `current` HashSet is only created when needed (write-on-create)
/// - Most child guards never allocate memory if they only read from parents
///
/// # Memory Layout
/// ```text
/// Root: current=[A, B] parent=None
///   |
///   +-- Child1: current=None parent=Root  (no allocation!)
///   |     |
///   |     +-- GrandChild: current=[C] parent=Child1  (allocated on first write)
///   |
///   +-- Child2: current=None parent=Root  (no allocation!)
/// ```
#[derive(Debug, Clone)]
pub struct InferGuard {
    /// Current level's visited types (lazily allocated)
    /// Only created when we need to add a new type not in parent chain
    current: RefCell<Option<HashSet<LuaTypeDeclId>>>,
    /// Parent guard (shared reference)
    parent: Option<Rc<InferGuard>>,
}

impl InferGuard {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            current: RefCell::new(None),
            parent: None,
        })
    }

    /// Create a child guard that inherits from parent
    ///
    /// Zero-cost operation: no HashSet allocation until first write
    pub fn fork(self: &Rc<Self>) -> Rc<Self> {
        Rc::new(Self {
            current: RefCell::new(None), // Lazy allocation
            parent: Some(Rc::clone(self)),
        })
    }

    /// Check if a type has been visited in current branch or any parent
    pub fn check(&self, type_id: &LuaTypeDeclId) -> Result<(), InferFailReason> {
        // Check in all parent levels first
        if self.contains_in_parents(type_id) {
            return Err(InferFailReason::RecursiveInfer);
        }

        // Check in current level (if exists)
        let mut current_opt = self.current.borrow_mut();

        // Lazy allocation: create HashSet only when needed
        let current = current_opt.get_or_insert_with(HashSet::default);

        if current.contains(type_id) {
            return Err(InferFailReason::RecursiveInfer);
        }

        // Mark as visited in current level
        current.insert(type_id.clone());
        Ok(())
    }

    /// Check if a type has been visited in parent chain
    fn contains_in_parents(&self, type_id: &LuaTypeDeclId) -> bool {
        let mut current_parent = self.parent.as_ref();
        while let Some(parent) = current_parent {
            if let Some(ref set) = *parent.current.borrow() {
                if set.contains(type_id) {
                    return true;
                }
            }
            current_parent = parent.parent.as_ref();
        }
        false
    }

    /// Check if a type has been visited (without modifying the guard)
    pub fn contains(&self, type_id: &LuaTypeDeclId) -> bool {
        // Check current level
        if let Some(ref set) = *self.current.borrow() {
            if set.contains(type_id) {
                return true;
            }
        }
        // Check parents
        self.contains_in_parents(type_id)
    }

    /// Get the depth of current level
    pub fn current_depth(&self) -> usize {
        self.current.borrow().as_ref().map_or(0, |set| set.len())
    }

    /// Get the total depth of the entire guard chain
    pub fn total_depth(&self) -> usize {
        let mut depth = self.current_depth();
        let mut current_parent = self.parent.as_ref();
        while let Some(parent) = current_parent {
            depth += parent.current_depth();
            current_parent = parent.parent.as_ref();
        }
        depth
    }

    /// Get the level of the guard chain (how many parents)
    pub fn level(&self) -> usize {
        let mut level = 0;
        let mut current_parent = self.parent.as_ref();
        while let Some(parent) = current_parent {
            level += 1;
            current_parent = parent.parent.as_ref();
        }
        level
    }

    /// Check if current level has allocated memory
    /// Useful for debugging and performance analysis
    #[cfg(test)]
    pub fn has_allocated(&self) -> bool {
        self.current.borrow().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_allocation() {
        let root = InferGuard::new();
        assert!(!root.has_allocated(), "New guard should not allocate");

        // Fork should NOT allocate
        let child = root.fork();
        assert!(!child.has_allocated(), "Fork should not allocate memory");

        // Check on child should allocate
        let type_b = LuaTypeDeclId::global("TestTypeB");
        child.check(&type_b).unwrap();
        assert!(
            child.has_allocated(),
            "Check should trigger lazy allocation"
        );
        assert!(!root.has_allocated(), "Root should not be affected");
    }

    #[test]
    fn test_fork_without_write() {
        let root = InferGuard::new();
        let type_a = LuaTypeDeclId::global("TestTypeA");
        root.check(&type_a).unwrap();

        // Create multiple forks
        let child1 = root.fork();
        let child2 = root.fork();
        let grandchild = child1.fork();

        // None of them should allocate if they don't write
        assert!(!child1.has_allocated());
        assert!(!child2.has_allocated());
        assert!(!grandchild.has_allocated());

        // They should still see parent's types
        assert!(child1.contains(&type_a));
        assert!(child2.contains(&type_a));
        assert!(grandchild.contains(&type_a));
    }

    #[test]
    fn test_recursive_detection() {
        let root = InferGuard::new();
        let type_a = LuaTypeDeclId::global("TestTypeA");

        // First check should succeed
        assert!(root.check(&type_a).is_ok());

        // Second check should fail (recursive)
        assert!(root.check(&type_a).is_err());
    }

    #[test]
    fn test_parent_chain_detection() {
        let root = InferGuard::new();
        let type_a = LuaTypeDeclId::global("TestTypeA");
        let type_b = LuaTypeDeclId::global("TestTypeB");

        root.check(&type_a).unwrap();

        let child = root.fork();

        // Child should detect type_a from parent
        assert!(child.check(&type_a).is_err());

        // But can add type_b
        assert!(child.check(&type_b).is_ok());

        let grandchild = child.fork();

        // Grandchild should detect both
        assert!(grandchild.check(&type_a).is_err());
        assert!(grandchild.check(&type_b).is_err());
    }

    #[test]
    fn test_memory_efficiency() {
        let root = InferGuard::new();
        let type_a = LuaTypeDeclId::global("TestTypeA");
        root.check(&type_a).unwrap();

        // Create a deep fork chain
        let mut guards = vec![root];
        for _ in 0..10 {
            let child = guards.last().unwrap().fork();
            guards.push(child);
        }

        // Only root and last guard (if it wrote) should have allocation
        // All intermediate forks should have NO allocation
        for (i, guard) in guards.iter().enumerate() {
            if i == 0 {
                assert!(guard.has_allocated(), "Root should be allocated");
            } else if i < guards.len() - 1 {
                // Intermediate nodes that didn't write
                assert!(
                    !guard.has_allocated(),
                    "Intermediate fork {} should not allocate",
                    i
                );
            }
        }
    }
}
