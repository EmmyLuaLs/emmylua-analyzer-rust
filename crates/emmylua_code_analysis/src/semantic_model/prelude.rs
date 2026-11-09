//! Shared imports for the implementation submodules of [`super`].
//!
//! Keeping these re-exports here lets `mod.rs` stay a minimal interface while
//! avoiding a large, duplicated import header in every implementation module.

pub(crate) use std::collections::HashSet;
pub(crate) use std::rc::Rc;

pub(crate) use emmylua_parser::{
    LuaAssignStat, LuaAstNode, LuaCallExpr, LuaChunk, LuaClosureExpr, LuaCommentOwner,
    LuaDocConditionalType, LuaDocNameType, LuaDocObjectFieldKey, LuaDocTag, LuaDocTagAlias,
    LuaDocTagClass, LuaDocType, LuaExpr, LuaForRangeStat, LuaIndexExpr, LuaLiteralExpr,
    LuaLiteralToken, LuaParseError, LuaSyntaxId, LuaSyntaxKind, LuaSyntaxNode, LuaSyntaxToken,
    LuaSyntaxTree, LuaTableExpr, LuaTableField, LuaTypeBinaryOperator, LuaTypeUnaryOperator,
    LuaVersionNumber, NumberResult, VisibilityKind,
};
pub(crate) use hashbrown::HashMap;
pub(crate) use rowan::TextSize;
pub(crate) use smol_str::SmolStr;

pub(crate) use crate::LuaDocument;
pub(crate) use crate::LuaType;
pub(crate) use crate::LuaTypeNode;
pub(crate) use crate::member_key::LuaMemberKey;

pub(crate) use crate::semantic_db::def::{
    ConstructorAttribute, Decl, DeclKind, Member, MemberRef, ModuleExport, NameUse, Scope,
    SemanticId, Signature, TypeDef, TypeDefKind, TypeScope,
};
pub(crate) use crate::semantic_db::flow::FlowTree;
pub(crate) use crate::signature::LuaSignatureId;
pub(crate) use crate::{
    AsyncState, DocGenericParam, FileExports, FileFacts, FileId, GenericParam, GenericTpl,
    GenericTplId, LuaAliasCallKind, LuaAliasCallType, LuaFunctionType, LuaIntersectionType,
    LuaObjectType, LuaTupleStatus, LuaTupleType, LuaTypeDeclId, LuaUnionType, VariadicType,
    WorkspaceId,
};

pub(crate) use super::{CallSiteAnalysis, ResolvedMember, SemanticInfo, SemanticModel};
pub(crate) use super::{cache, flow, infer, member, type_check, type_eval};
