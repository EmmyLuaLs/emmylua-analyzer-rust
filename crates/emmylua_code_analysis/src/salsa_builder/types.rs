//! Fixed-point domain for type inference.
//!
//! Inference is a monotonic fixed point. salsa's `cycle_fn` is a fixed-point iterator, but
//! **convergence must happen on a bounded value domain**. The candidate set (sorted/deduped) is from a finite set,
//! so it converges and cannot grow unboundedly like structural types.

use std::sync::Arc;

use smol_str::SmolStr;

use crate::FileId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrimitiveType {
    Nil,
    Boolean,
    Integer,
    Number,
    String,
    Table,
    Function,
    /// Empty object literal `{}` in docs: structurally an "empty table", not a broad `table`.
    EmptyObject,
}

/// Synthetic identity for anonymous table literals (file + table syntax range). `Ord`; rebuilt when converting to `SemanticId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId {
    pub file_id: u32,
    pub start: u32,
    pub end: u32,
}

impl TableId {
    pub fn from_range(file_id: FileId, range: rowan::TextRange) -> Self {
        TableId {
            file_id: file_id.id,
            start: u32::from(range.start()),
            end: u32::from(range.end()),
        }
    }
}

/// Structured function type (`fun<T>(a: number): string`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionShell {
    pub params: Vec<TypeShell>,
    /// Parameter names (aligned with params; `...` is variadic).
    pub param_names: Vec<SmolStr>,
    pub returns: TypeShell,
    /// Per-slot return types (`---@return number, string` = 2 slots).
    pub returns_multi: Vec<TypeShell>,
    /// Generic parameter names for `fun<T, U>(...)` (`T` inside parameters → `Generic("T")`; `TplRef` is built during projection).
    pub generic_params: Vec<SmolStr>,
    /// `async fun(...)` / `sync fun(...)`: 0 = None, 1 = Async, 2 = Sync.
    pub async_state: u8,
    /// `fun(self: ...)` / method definition.
    pub is_colon_define: bool,
    /// Parameters have `...`.
    pub is_variadic: bool,
}

/// Literal constant candidates (bounded: constants appearing in program/docs).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LiteralShell {
    String(SmolStr),
    Integer(i64),
    /// f64 bit pattern (f64 has no Ord; bitwise comparison keeps the domain bounded).
    Float(u64),
    Boolean(bool),
    Nil,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StrTplRef {
    pub prefix: SmolStr,
    pub name: SmolStr,
    pub tpl_index: Option<u32>,
    pub suffix: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GenericInstanceRef {
    pub name: SmolStr,
    pub args: Vec<TypeShell>,
}

/// Type candidates. Bounded: primitives / name strings / generic names / synthetic table ids / generic instances (sets appearing in the program).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeCandidate {
    Primitive(PrimitiveType),
    Named(SmolStr),
    /// Generic parameter reference (`T` bound to GenericParam; represented by name before instantiation).
    Generic(SmolStr),
    /// Anonymous table literal (members collected under a synthetic owner).
    Table(TableId),
    /// Generic instantiation: `Box<number>` → base type name + argument types.
    GenericInstance(Arc<GenericInstanceRef>),
    /// Array: `T[]` → base type (the carrier used by unify to infer `T`).
    Array(Arc<TypeShell>),
    /// Variadic: `T...` → base type.
    Variadic(Arc<TypeShell>),
    /// Tuple: `[number, string]` → per-slot types.
    Tuple(Vec<TypeShell>),
    Function(Arc<FunctionShell>),
    /// String template type (`` `T` `` / `` `prefix.T` ``): string arguments replace placeholder names.
    StrTpl(Arc<StrTplRef>),
    /// Literal constants (`1` / `"a"` / `true` / `nil`).
    Literal(LiteralShell),
    /// `---@module "name"` annotation: module reference (points directly to the target file).
    ModuleRef(FileId),
    #[allow(unused)]
    Recursive,
}

/// Sorted, deduplicated candidate set — the carrier for fixed-point convergence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::SalsaValue)]
pub struct TypeShell {
    pub candidates: Vec<TypeCandidate>,
}

impl TypeShell {
    /// Unknown = empty set (union identity element; never pollutes results).
    pub fn unknown() -> Self {
        Self {
            candidates: Vec::new(),
        }
    }

    #[allow(unused)]
    pub fn recursive() -> Self {
        Self {
            candidates: vec![TypeCandidate::Recursive],
        }
    }

    pub fn from_primitive(p: PrimitiveType) -> Self {
        Self {
            candidates: vec![TypeCandidate::Primitive(p)],
        }
    }

    pub fn from_name(name: &str) -> Self {
        Self {
            candidates: vec![TypeCandidate::Named(SmolStr::new(name))],
        }
    }

    /// Generic parameter reference candidate.
    pub fn from_generic(name: &str) -> Self {
        Self {
            candidates: vec![TypeCandidate::Generic(SmolStr::new(name))],
        }
    }

    /// Structured function type candidate.
    pub fn from_function(
        params: Vec<TypeShell>,
        param_names: Vec<SmolStr>,
        returns: TypeShell,
        returns_multi: Vec<TypeShell>,
        generic_params: Vec<SmolStr>,
        async_state: u8,
        is_colon_define: bool,
        is_variadic: bool,
    ) -> Self {
        Self {
            candidates: vec![TypeCandidate::Function(Arc::new(FunctionShell {
                params,
                param_names,
                returns,
                returns_multi,
                generic_params,
                async_state,
                is_colon_define,
                is_variadic,
            }))],
        }
    }

    /// Anonymous table literal candidate (synthetic owner).
    pub fn from_table(table_id: TableId) -> Self {
        Self {
            candidates: vec![TypeCandidate::Table(table_id)],
        }
    }

    /// Generic instantiation candidate: `Box<number>`.
    pub fn from_generic_instance(name: &str, args: Vec<TypeShell>) -> Self {
        Self {
            candidates: vec![TypeCandidate::GenericInstance(Arc::new(
                GenericInstanceRef {
                    name: SmolStr::new(name),
                    args,
                },
            ))],
        }
    }

    /// Array candidate: `T[]`.
    pub fn from_array(base: TypeShell) -> Self {
        Self {
            candidates: vec![TypeCandidate::Array(Arc::new(base))],
        }
    }

    /// Variadic candidate: `T...`.
    pub fn from_variadic(base: TypeShell) -> Self {
        Self {
            candidates: vec![TypeCandidate::Variadic(Arc::new(base))],
        }
    }

    /// Tuple candidate: `[number, string]`.
    pub fn from_tuple(types: Vec<TypeShell>) -> Self {
        Self {
            candidates: vec![TypeCandidate::Tuple(types)],
        }
    }

    /// Literal constant candidate.
    pub fn from_literal(literal: LiteralShell) -> Self {
        Self {
            candidates: vec![TypeCandidate::Literal(literal)],
        }
    }

    /// Module reference candidate (`---@module "name"`).
    pub fn from_module_ref(file_id: FileId) -> Self {
        Self {
            candidates: vec![TypeCandidate::ModuleRef(file_id)],
        }
    }

    /// String template candidate: `` `T` `` (string arguments replace placeholder names).
    pub fn from_str_tpl(prefix: &str, name: &str, tpl_index: Option<u32>, suffix: &str) -> Self {
        Self {
            candidates: vec![TypeCandidate::StrTpl(Arc::new(StrTplRef {
                prefix: SmolStr::new(prefix),
                name: SmolStr::new(name),
                tpl_index,
                suffix: SmolStr::new(suffix),
            }))],
        }
    }

    pub fn is_unknown(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Union (sorted and deduplicated; monotone, so convergence is guaranteed).
    pub fn merge(&mut self, other: &TypeShell) {
        for candidate in &other.candidates {
            if !self.candidates.contains(candidate) {
                self.candidates.push(candidate.clone());
            }
        }
        self.candidates.sort();
        self.candidates.dedup();
    }
}
