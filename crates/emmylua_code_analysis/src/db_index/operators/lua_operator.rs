use std::sync::Arc;

use rowan::{TextRange, TextSize};

use crate::{
    AsyncState, DbIndex, FileId, InFiled, InferFailReason, LuaConstructorReturnMode,
    LuaFunctionType, LuaSignature, LuaSignatureId, SignatureReturnStatus,
    db_index::{LuaType, LuaTypeDeclId},
};

use super::lua_operator_meta_method::LuaOperatorMetaMethod;

#[derive(Debug)]
pub struct LuaOperator {
    owner: LuaOperatorOwner,
    op: LuaOperatorMetaMethod,
    file_id: FileId,
    range: TextRange,
    func: OperatorFunction,
}

#[derive(Debug, Clone)]
pub enum OperatorFunction {
    // One explicit parameter: `@operator add(T): R`, `@operator sub(T): R`, or `@field [K] V`.
    BinOp {
        param: LuaType,
        ret: LuaType,
    },
    // Unary declarations such as `@operator unm: R` or `@operator len: R`.
    UnOp {
        ret: LuaType,
    },
    // `@operator call(...)`.
    Call {
        params: Vec<LuaType>,
        ret: LuaType,
    },
    // `@overload fun(...)`.
    Overload(Arc<LuaFunctionType>),
    // Runtime metatable closures like `__add = function(...)` or `__call = function(...)`.
    Signature(LuaSignatureId),
    // Synthesized call operator for default class constructors.
    DefaultClassCtor {
        id: LuaSignatureId,
        strip_self: bool,
        return_mode: LuaConstructorReturnMode,
    },
}

impl LuaOperator {
    pub fn new(
        owner: LuaOperatorOwner,
        op: LuaOperatorMetaMethod,
        file_id: FileId,
        range: TextRange,
        func: OperatorFunction,
    ) -> Self {
        Self {
            owner,
            op,
            file_id,
            range,
            func,
        }
    }

    pub fn get_owner(&self) -> &LuaOperatorOwner {
        &self.owner
    }

    pub fn get_op(&self) -> LuaOperatorMetaMethod {
        self.op
    }

    pub fn get_operand(&self, db: &DbIndex) -> LuaType {
        match &self.func {
            OperatorFunction::BinOp { param, .. } => param.clone(),
            OperatorFunction::Signature(signature) => {
                let signature = db.get_signature_index().get(signature);
                if let Some(signature) = signature {
                    let param = signature.get_param_info_by_id(1);
                    if let Some(param) = param {
                        return param.type_ref.clone();
                    }
                }

                LuaType::Any
            }
            OperatorFunction::Overload(_)
            | OperatorFunction::UnOp { .. }
            | OperatorFunction::Call { .. }
            | OperatorFunction::DefaultClassCtor { .. } => LuaType::Unknown,
        }
    }

    pub fn get_result(&self, db: &DbIndex) -> Result<LuaType, InferFailReason> {
        match &self.func {
            OperatorFunction::BinOp { ret, .. }
            | OperatorFunction::UnOp { ret }
            | OperatorFunction::Call { ret, .. } => Ok(ret.clone()),
            OperatorFunction::Overload(func) => Ok(func.get_ret().clone()),
            OperatorFunction::Signature(signature_id) => {
                let signature = db.get_signature_index().get(signature_id);
                if let Some(signature) = signature {
                    if signature.resolve_return == SignatureReturnStatus::UnResolve {
                        return Err(InferFailReason::UnResolveSignatureReturn(*signature_id));
                    }

                    let return_type = signature.return_docs.first();
                    if let Some(return_type) = return_type {
                        return Ok(return_type.type_ref.clone());
                    }
                }

                Ok(LuaType::Any)
            }
            OperatorFunction::DefaultClassCtor {
                id, return_mode, ..
            } => {
                let signature = db.get_signature_index().get(id);
                if let Some(signature) = signature {
                    let return_type = get_constructor_return_type(signature, return_mode);
                    return Ok(return_type);
                }

                Ok(LuaType::Any)
            }
        }
    }

    pub fn get_operator_func(&self, db: &DbIndex) -> LuaType {
        match &self.func {
            OperatorFunction::BinOp { param, ret } => LuaFunctionType::new(
                AsyncState::None,
                false,
                false,
                vec![
                    ("self".to_string(), Some(LuaType::SelfInfer)),
                    ("arg0".to_string(), Some(param.clone())),
                ],
                ret.clone(),
            )
            .into(),
            OperatorFunction::UnOp { ret } => LuaFunctionType::new(
                AsyncState::None,
                false,
                false,
                vec![("self".to_string(), Some(LuaType::SelfInfer))],
                ret.clone(),
            )
            .into(),
            OperatorFunction::Call { params, ret } => {
                let is_variadic = params.last().is_some_and(LuaType::is_variadic);
                let last_param_idx = params.len().saturating_sub(1);
                let params = params
                    .iter()
                    .enumerate()
                    .map(|(i, param)| {
                        let name = if is_variadic && i == last_param_idx {
                            "...".to_string()
                        } else {
                            format!("arg{}", i)
                        };
                        (name, Some(param.clone()))
                    })
                    .collect();

                LuaFunctionType::new(AsyncState::None, false, is_variadic, params, ret.clone())
                    .into()
            }
            OperatorFunction::Overload(func) => {
                LuaType::DocFunction(func.to_call_operator_func_type())
            }
            OperatorFunction::Signature(signature) => LuaType::Signature(*signature),
            OperatorFunction::DefaultClassCtor {
                id,
                strip_self,
                return_mode,
            } => match db.get_signature_index().get(id) {
                Some(signature) => LuaFunctionType::new(
                    signature.async_state,
                    !*strip_self && signature.is_colon_define,
                    signature.is_vararg,
                    signature.get_type_params(),
                    get_constructor_return_type(signature, return_mode),
                )
                .into(),
                None => LuaType::Signature(*id),
            },
        }
    }

    pub fn get_file_id(&self) -> FileId {
        self.file_id
    }

    pub fn get_id(&self) -> LuaOperatorId {
        LuaOperatorId {
            file_id: self.file_id,
            position: self.range.start(),
        }
    }

    pub fn get_range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuaOperatorId {
    pub file_id: FileId,
    pub position: TextSize,
}

impl LuaOperatorId {
    pub fn new(position: TextSize, file_id: FileId) -> Self {
        Self { position, file_id }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LuaOperatorOwner {
    Table(InFiled<TextRange>),
    Type(LuaTypeDeclId),
}

impl From<LuaTypeDeclId> for LuaOperatorOwner {
    fn from(id: LuaTypeDeclId) -> Self {
        LuaOperatorOwner::Type(id)
    }
}

impl From<InFiled<TextRange>> for LuaOperatorOwner {
    fn from(id: InFiled<TextRange>) -> Self {
        LuaOperatorOwner::Table(id)
    }
}

fn get_constructor_return_type(
    signature: &LuaSignature,
    return_mode: &LuaConstructorReturnMode,
) -> LuaType {
    let return_type = match *return_mode {
        LuaConstructorReturnMode::SelfType => LuaType::SelfInfer,
        LuaConstructorReturnMode::Doc => signature.get_return_type(),
        LuaConstructorReturnMode::Default => {
            if signature.resolve_return == SignatureReturnStatus::DocResolve {
                signature.get_return_type()
            } else {
                LuaType::SelfInfer
            }
        }
    };
    return_type
}
