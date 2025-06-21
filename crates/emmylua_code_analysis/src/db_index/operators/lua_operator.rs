use std::sync::Arc;

use rowan::{TextRange, TextSize};

use crate::{
    db_index::{LuaType, LuaTypeDeclId},
    DbIndex, FileId, InFiled, InferFailReason, LuaFunctionType, LuaSignatureId,
    SignatureReturnStatus,
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
    Func(Arc<LuaFunctionType>),
    Signature(LuaSignatureId),
    DefaultCall(LuaSignatureId),
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
            OperatorFunction::Func(func) => {
                let params = func.get_params();
                if params.len() >= 2 {
                    return params[1].1.clone().unwrap_or(LuaType::Any);
                }

                LuaType::Any
            }
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
            // 只有 .field 才有`operand`, call 不会有这个
            OperatorFunction::DefaultCall(_) => LuaType::Unknown,
        }
    }

    pub fn get_result(&self, db: &DbIndex) -> Result<LuaType, InferFailReason> {
        match &self.func {
            OperatorFunction::Func(func) => Ok(func.get_ret().clone()),
            OperatorFunction::Signature(signature_id) => {
                let signature = db.get_signature_index().get(signature_id);
                if let Some(signature) = signature {
                    if signature.resolve_return == SignatureReturnStatus::UnResolve {
                        return Err(InferFailReason::UnResolveSignatureReturn(
                            signature_id.clone(),
                        ));
                    }

                    let return_type = signature.return_docs.get(0);
                    if let Some(return_type) = return_type {
                        return Ok(return_type.type_ref.clone());
                    }
                }

                Ok(LuaType::Any)
            }
            OperatorFunction::DefaultCall(signature_id) => {
                let emmyrc = db.get_emmyrc();
                if emmyrc.runtime.class_default_call.force_return_self {
                    return Ok(LuaType::SelfInfer);
                }

                if let Some(signature) = db.get_signature_index().get(signature_id) {
                    if signature.resolve_return == SignatureReturnStatus::UnResolve {
                        return Err(InferFailReason::UnResolveSignatureReturn(
                            signature_id.clone(),
                        ));
                    }

                    let return_type = signature.return_docs.get(0);
                    if let Some(return_type) = return_type {
                        return Ok(return_type.type_ref.clone());
                    }
                }

                Ok(LuaType::Any)
            }
        }
    }

    pub fn get_operator_func(&self, db: &DbIndex) -> LuaType {
        match &self.func {
            OperatorFunction::Func(func) => LuaType::DocFunction(func.clone()),
            OperatorFunction::Signature(signature) => LuaType::Signature(*signature),
            OperatorFunction::DefaultCall(signature_id) => {
                let emmyrc = db.get_emmyrc();

                if let Some(signature) = db.get_signature_index().get(signature_id) {
                    let params = signature.get_type_params();
                    let is_colon_define = if emmyrc.runtime.class_default_call.force_non_colon {
                        false
                    } else {
                        signature.is_colon_define
                    };
                    let return_type = if emmyrc.runtime.class_default_call.force_return_self {
                        LuaType::SelfInfer
                    } else {
                        signature.get_return_type()
                    };
                    let func_type = LuaFunctionType::new(
                        signature.is_async,
                        is_colon_define,
                        params,
                        return_type,
                    );
                    return LuaType::DocFunction(Arc::new(func_type));
                }

                LuaType::Signature(*signature_id)
            }
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
