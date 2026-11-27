//! Allocation regression guard for canonical union equality.
//!
//! Union types are used as `HashMap` keys by the semantic caches, so equality
//! and hashing must stay allocation-free on the common (small union) path.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use emmylua_code_analysis::{LuaType, LuaTypeDeclId};

struct CountingAlloc;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

fn union() -> LuaType {
    LuaType::union(vec![
        LuaType::Ref(LuaTypeDeclId::global("UnionAllocA")),
        LuaType::Ref(LuaTypeDeclId::global("UnionAllocB")),
    ])
}

#[test]
fn union_eq_and_hash_do_not_allocate() {
    let left = union();
    let right = union();
    assert_eq!(left, right);

    // Warm any lazily cached state (e.g. the union's member hash).
    let mut hasher = DefaultHasher::new();
    left.hash(&mut hasher);
    let _ = hasher.finish();
    let mut hasher = DefaultHasher::new();
    right.hash(&mut hasher);
    let _ = hasher.finish();

    ALLOCS.store(0, Ordering::Relaxed);
    for _ in 0..1_000 {
        assert!(black_box(&left) == black_box(&right));
    }
    let eq_allocations = ALLOCS.load(Ordering::Relaxed);

    ALLOCS.store(0, Ordering::Relaxed);
    for _ in 0..1_000 {
        let mut hasher = DefaultHasher::new();
        black_box(&left).hash(&mut hasher);
        black_box(hasher.finish());
    }
    let hash_allocations = ALLOCS.load(Ordering::Relaxed);

    ALLOCS.store(0, Ordering::Relaxed);
    for _ in 0..1_000 {
        let _ = black_box(&left).filter_components(|_| true);
    }
    let filter_allocations = ALLOCS.load(Ordering::Relaxed);

    assert_eq!(eq_allocations, 0, "union equality allocated");
    assert_eq!(hash_allocations, 0, "union hashing allocated");
    assert_eq!(
        filter_allocations, 0,
        "a no-op union component filter allocated"
    );

    // Construction should not need an intermediate `HashSet` or result Vec:
    // the caller's Vec plus the union's Rc allocation are the only two.
    let ref_a = LuaType::Ref(LuaTypeDeclId::global("UnionAllocA"));
    let ref_b = LuaType::Ref(LuaTypeDeclId::global("UnionAllocB"));

    ALLOCS.store(0, Ordering::Relaxed);
    let _ = LuaType::union(vec![LuaType::String, LuaType::Nil]);
    let basic_allocations = ALLOCS.load(Ordering::Relaxed);

    ALLOCS.store(0, Ordering::Relaxed);
    let _ = LuaType::union(vec![ref_a.clone(), LuaType::Nil]);
    let nullable_allocations = ALLOCS.load(Ordering::Relaxed);

    ALLOCS.store(0, Ordering::Relaxed);
    let _ = LuaType::union(vec![ref_a.clone(), ref_b]);
    let members_allocations = ALLOCS.load(Ordering::Relaxed);

    ALLOCS.store(0, Ordering::Relaxed);
    let _ = LuaType::union(vec![ref_a.clone(), ref_a]);
    let collapse_allocations = ALLOCS.load(Ordering::Relaxed);

    assert!(
        basic_allocations <= 2,
        "basic union allocated {basic_allocations} times"
    );
    assert!(
        nullable_allocations <= 2,
        "nullable union allocated {nullable_allocations} times"
    );
    assert!(
        members_allocations <= 2,
        "member union allocated {members_allocations} times"
    );
    assert!(
        collapse_allocations <= 1,
        "a duplicate-only union should collapse without an Rc allocation, got {collapse_allocations}"
    );
}
