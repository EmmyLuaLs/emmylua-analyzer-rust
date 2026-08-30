//! # def - globally unique "definition" types
//!
//! Separated from `facts`: `facts` is the file arena (containers + extraction); here are the shapes and identities of definitions.
//! Each definition carries `id: SemanticId` (globally unique, <=16 bytes) to describe reference / parent / member relationships.

mod async_state;
mod builtin_attribute;
pub mod decl;
pub mod generic_param;
mod lua_type;
pub mod member;
pub mod member_key;
pub mod module_export;
mod module_info;
pub mod module_visibility;
pub mod name_use;
pub mod operator;
mod render_level;
pub mod scope;
pub mod semantic_id;
pub mod signature;
pub mod type_def;
mod workspace_id;

pub use async_state::AsyncState;
pub use builtin_attribute::*;
pub use decl::{Decl, DeclKind};
pub use generic_param::SalsaGenericParam;
pub use lua_type::*;
pub use member::{Member, MemberRef};
pub use member_key::LuaMemberKey;
pub use module_export::ModuleExport;
pub use module_info::{ModuleInfo, ModuleNode, ModuleNodeId};
pub use module_visibility::ModuleVisibility;
pub use name_use::NameUse;
pub use operator::OperatorDef;
pub use render_level::RenderLevel;
pub use scope::{Scope, ScopeChild, ScopeKind};
pub use semantic_id::SemanticId;
pub use signature::{
    ConstructorAttribute, ConstructorReturnMode, LuaSignatureId, Signature, SignatureDoc,
    SignatureReturnCast,
};
pub use type_def::{TypeDef, TypeDefFlags, TypeDefKind, TypeScope, TypeVisibility};
pub use workspace_id::WorkspaceId;
