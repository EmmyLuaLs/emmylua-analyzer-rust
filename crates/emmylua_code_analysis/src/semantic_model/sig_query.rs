//! SigQuery — 签名与调用查询 + lowered type 解析
//!
//! 提供签名、调用解释、泛型约束解析、Lowered type → LuaType 转换。

use std::sync::Arc;

use rowan::TextSize;
use smol_str::SmolStr;

use crate::compilation::{
    SalsaCallExplainSummary, SalsaDocTypeDefSummary, SalsaDocTypeLoweredKind,
    SalsaDocTypeLoweredNode, SalsaDocTypeRef, SalsaSignatureExplainSummary,
    SalsaSignatureIndexSummary, SalsaSummaryDatabase,
};
use crate::semantic_model::infer;
use crate::semantic_model::signature::SignatureInfo;
use crate::{
    AsyncState, FileId, LuaFunctionType, LuaGenericType, LuaIndexAccessKey, LuaObjectType, LuaType,
    LuaTypeDeclId, SalsaDocTypeLoweredObjectFieldKey,
};

/// 签名与调用查询器。
pub struct SigQuery<'db> {
    db: &'db SalsaSummaryDatabase,
    file_id: FileId,
}

impl<'db> SigQuery<'db> {
    pub(crate) fn new(db: &'db SalsaSummaryDatabase, file_id: FileId) -> Self {
        Self { db, file_id }
    }

    pub(crate) fn file_id(&self) -> FileId {
        self.file_id
    }

    pub(crate) fn db(&self) -> &SalsaSummaryDatabase {
        &self.db
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 签名查询
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn all(&self) -> Option<Arc<SalsaSignatureIndexSummary>> {
        let db = self.db();
        db.doc().signature().summary(self.file_id)
    }

    pub fn get(&self, file_id: FileId, offset: TextSize) -> Option<SignatureInfo> {
        let db = self.db();
        SignatureInfo::query(&db, file_id, offset)
    }

    pub fn explain(
        &self,
        file_id: FileId,
        offset: TextSize,
    ) -> Option<SalsaSignatureExplainSummary> {
        let db = self.db();
        db.doc().signature().explain(file_id, offset)
    }

    pub fn call_explain(&self, call_offset: TextSize) -> Option<SalsaCallExplainSummary> {
        let db = self.db();
        db.doc().signature().call_explain(self.file_id, call_offset)
    }

    /// 递归解析 lowered type 为 LuaType（支持所有 lowered kind）。
    pub fn resolve_lowered(&self, node: &SalsaDocTypeLoweredNode) -> Option<LuaType> {
        self.resolve_lowered_inner(node, 0)
    }

    /// 保持向后兼容的简单转换（仅支持 Name/Array/Variadic/Literal）。
    pub fn lowered_to_type(&self, lowered: &SalsaDocTypeLoweredNode) -> Option<LuaType> {
        infer::lowered_node_to_lua_type(lowered)
    }

    /// 递归解析，depth 防无限递归。
    fn resolve_lowered_inner(&self, node: &SalsaDocTypeLoweredNode, depth: u32) -> Option<LuaType> {
        if depth > 32 {
            return None;
        }

        match &node.kind {
            // ── 基础类型 ──
            SalsaDocTypeLoweredKind::Unknown => Some(LuaType::Any),

            SalsaDocTypeLoweredKind::Name { name } => Self::resolve_name(name.as_str()),

            SalsaDocTypeLoweredKind::Infer { generic_name: _ } => {
                // 泛型引用 — 调用者需要提供上下文来解析
                Some(LuaType::Any)
            }

            SalsaDocTypeLoweredKind::Literal { text } => {
                let s = text.as_str();
                match s {
                    "nil" => Some(LuaType::Nil),
                    "true" => Some(LuaType::BooleanConst(true)),
                    "false" => Some(LuaType::BooleanConst(false)),
                    _ => {
                        if let Ok(n) = s.parse::<i64>() {
                            Some(LuaType::IntegerConst(n))
                        } else if let Ok(f) = s.parse::<f64>() {
                            Some(LuaType::FloatConst(f))
                        } else {
                            Some(LuaType::DocStringConst(SmolStr::new(s).into()))
                        }
                    }
                }
            }

            // ── 复合类型 ──
            SalsaDocTypeLoweredKind::Array { item_type } => {
                let inner = self
                    .resolve_type_ref(item_type, depth + 1)
                    .unwrap_or(LuaType::Any);
                Some(LuaType::Array(
                    crate::LuaArrayType::new(inner, crate::LuaArrayLen::None).into(),
                ))
            }

            SalsaDocTypeLoweredKind::Variadic { item_type } => {
                let inner = self
                    .resolve_type_ref(item_type, depth + 1)
                    .unwrap_or(LuaType::Any);
                Some(LuaType::Variadic(crate::VariadicType::Base(inner).into()))
            }

            SalsaDocTypeLoweredKind::Nullable { inner_type } => {
                self.resolve_type_ref(inner_type, depth + 1)
            }

            SalsaDocTypeLoweredKind::Union(item_types) => {
                let types: Vec<LuaType> = item_types
                    .iter()
                    .filter_map(|t| self.resolve_type_ref(t, depth + 1))
                    .collect();
                match types.len() {
                    0 => None,
                    1 => types.into_iter().next(),
                    _ => Some(LuaType::Union(crate::LuaUnionType::from_vec(types).into())),
                }
            }

            SalsaDocTypeLoweredKind::Intersection(item_types) => {
                let types: Vec<LuaType> = item_types
                    .iter()
                    .filter_map(|t| self.resolve_type_ref(t, depth + 1))
                    .collect();
                match types.len() {
                    0 => None,
                    1 => types.into_iter().next(),
                    _ => Some(LuaType::Intersection(
                        crate::LuaIntersectionType::new(types).into(),
                    )),
                }
            }

            SalsaDocTypeLoweredKind::Tuple(item_types) => {
                let types: Vec<LuaType> = item_types
                    .iter()
                    .filter_map(|t| self.resolve_type_ref(t, depth + 1))
                    .collect();
                if types.is_empty() {
                    None
                } else {
                    Some(LuaType::Tuple(
                        crate::LuaTupleType::new(types, crate::LuaTupleStatus::DocResolve).into(),
                    ))
                }
            }

            SalsaDocTypeLoweredKind::Function(body) => {
                let func_params: Vec<(String, Option<LuaType>)> = body
                    .params
                    .iter()
                    .map(|p| {
                        let ty = self.resolve_type_ref(&p.doc_type, depth + 1);
                        (
                            p.name.clone().map(|n| n.to_string()).unwrap_or_default(),
                            ty,
                        )
                    })
                    .collect();
                let is_variadic = body.params.last().is_some_and(|p| p.is_dots);
                let ret = body
                    .returns
                    .first()
                    .and_then(|r| self.resolve_type_ref(&r.doc_type, depth + 1))
                    .unwrap_or(LuaType::Nil);
                // Detect colon define: first param named "self" implies implicit self
                let is_colon = func_params
                    .first()
                    .map(|(name, _)| name == "self")
                    .unwrap_or(false);
                let func_type = LuaFunctionType::new(
                    AsyncState::None,
                    is_colon,
                    is_variadic,
                    func_params,
                    ret,
                    None,
                );
                Some(LuaType::DocFunction(func_type.into()))
            }

            SalsaDocTypeLoweredKind::Object(body) => {
                let mut obj_fields: Vec<(LuaIndexAccessKey, LuaType)> = Vec::new();
                for f in body.fields.iter() {
                    let ty = self
                        .resolve_type_ref(&f.value_type, depth + 1)
                        .unwrap_or(LuaType::Any);
                    let key = match &f.key {
                        SalsaDocTypeLoweredObjectFieldKey::Name(n)
                        | SalsaDocTypeLoweredObjectFieldKey::String(n) => {
                            LuaIndexAccessKey::String(SmolStr::new(n.as_str()))
                        }
                        SalsaDocTypeLoweredObjectFieldKey::Integer(i) => {
                            LuaIndexAccessKey::Integer(*i)
                        }
                        _ => continue,
                    };
                    obj_fields.push((key, ty));
                }
                Some(LuaType::Object(LuaObjectType::new(obj_fields).into()))
            }

            SalsaDocTypeLoweredKind::Generic(body) => {
                let base = self.resolve_type_ref(&body.0, depth + 1)?;
                let args: Vec<LuaType> = body.1
                    .iter()
                    .filter_map(|t| self.resolve_type_ref(t, depth + 1))
                    .collect();
                if args.is_empty() {
                    Some(base)
                } else if let LuaType::Ref(type_id) = &base {
                    Some(LuaType::Generic(
                        LuaGenericType::new(type_id.clone(), args).into(),
                    ))
                } else {
                    Some(base)
                }
            }

            SalsaDocTypeLoweredKind::StringTemplate { .. } => {
                // StringTemplate 需要 GenericTplId，简化处理
                Some(LuaType::String)
            }

            // ── 未实现的 kind ──
            SalsaDocTypeLoweredKind::Binary(_)
            | SalsaDocTypeLoweredKind::Unary(_)
            | SalsaDocTypeLoweredKind::Conditional(_)
            | SalsaDocTypeLoweredKind::MultiLineUnion(_)
            | SalsaDocTypeLoweredKind::Attribute(_)
            | SalsaDocTypeLoweredKind::Mapped(_)
            | SalsaDocTypeLoweredKind::IndexAccess(_) => None,
        }
    }

    /// 解析 type reference（Node key → lowered node → resolve）
    fn resolve_type_ref(&self, ref_: &SalsaDocTypeRef, depth: u32) -> Option<LuaType> {
        match ref_ {
            SalsaDocTypeRef::Node(key) => {
                let db = self.db();
                // 在当前文件中查找 lowered type
                if let Some(resolved) = db.doc().resolved_type_by_key(self.file_id, *key) {
                    return self.resolve_lowered_inner(&resolved.lowered, depth);
                }
                // 跨文件查找
                for fid in db.file_ids() {
                    if let Some(resolved) = db.doc().resolved_type_by_key(fid, *key) {
                        return self.resolve_lowered_inner(&resolved.lowered, depth);
                    }
                }
                None
            }
            SalsaDocTypeRef::Incomplete => None,
        }
    }

    /// 名称 → LuaType（内置类型 + 自定义类型）
    fn resolve_name(name: &str) -> Option<LuaType> {
        match name {
            "any" | "unknown" => Some(LuaType::Any),
            "nil" => Some(LuaType::Nil),
            "false" => Some(LuaType::BooleanConst(false)),
            "true" => Some(LuaType::BooleanConst(true)),
            "boolean" | "bool" => Some(LuaType::Boolean),
            "string" => Some(LuaType::String),
            "number" => Some(LuaType::Number),
            "integer" | "int" => Some(LuaType::Integer),
            "function" => Some(LuaType::Function),
            "table" => Some(LuaType::Table),
            "thread" => Some(LuaType::Thread),
            "userdata" => Some(LuaType::Userdata),
            _ => Some(LuaType::Ref(LuaTypeDeclId::global(name))),
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 泛型约束解析
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    pub fn type_generic_constraints(
        &self,
        type_def: &SalsaDocTypeDefSummary,
        type_def_file_id: FileId,
    ) -> Vec<(Option<LuaType>, Option<LuaType>)> {
        let db = self.db();

        let raw: Vec<RawGenericParam> = type_def
            .generic_params
            .iter()
            .map(|param| {
                let constraint_lowered = param.type_offset.and_then(|key| {
                    db.doc()
                        .resolved_type_by_key(type_def_file_id, key)
                        .map(|r| r.lowered)
                });
                let default_lowered = param.default_type_offset.and_then(|key| {
                    db.doc()
                        .resolved_type_by_key(type_def_file_id, key)
                        .map(|r| r.lowered)
                });
                RawGenericParam {
                    name: param.name.to_string(),
                    constraint_lowered,
                    default_lowered,
                }
            })
            .collect();

        raw.iter()
            .map(|rp| {
                let constraint = rp
                    .constraint_lowered
                    .as_ref()
                    .and_then(|lt| resolve_lowered_with_infer(lt, &raw));
                let default = rp
                    .default_lowered
                    .as_ref()
                    .and_then(|lt| resolve_lowered_with_infer(lt, &raw));
                (constraint, default)
            })
            .collect()
    }

    pub fn signature_generic_constraints(
        &self,
        explain: &SalsaSignatureExplainSummary,
    ) -> Vec<(Option<LuaType>, Option<LuaType>)> {
        explain
            .generics
            .iter()
            .flat_map(|g| &g.params)
            .map(|param| {
                let constraint = param
                    .bound_type
                    .as_ref()
                    .and_then(|bt| infer::lowered_node_to_lua_type(bt.lowered.as_ref()?));
                let default = param
                    .default_type
                    .as_ref()
                    .and_then(|dt| infer::lowered_node_to_lua_type(dt.lowered.as_ref()?));
                (constraint, default)
            })
            .collect()
    }
}

struct RawGenericParam {
    name: String,
    constraint_lowered: Option<SalsaDocTypeLoweredNode>,
    default_lowered: Option<SalsaDocTypeLoweredNode>,
}

fn resolve_lowered_with_infer(
    lowered: &SalsaDocTypeLoweredNode,
    raw_params: &[RawGenericParam],
) -> Option<LuaType> {
    match &lowered.kind {
        SalsaDocTypeLoweredKind::Infer { generic_name } => raw_params
            .iter()
            .find(|rp| rp.name == generic_name.as_str())
            .and_then(|rp| {
                rp.constraint_lowered.as_ref().and_then(|lt| {
                    if matches!(&lt.kind, SalsaDocTypeLoweredKind::Infer { .. }) {
                        None
                    } else {
                        infer::lowered_node_to_lua_type(lt)
                    }
                })
            }),
        _ => infer::lowered_node_to_lua_type(lowered),
    }
}
