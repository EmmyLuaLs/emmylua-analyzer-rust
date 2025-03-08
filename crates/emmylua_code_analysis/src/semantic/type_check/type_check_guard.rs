use super::type_check_fail_reason::TypeCheckFailReason;

const MAX_TYPE_CHECK_LEVEL: i32 = 100;
pub type TypeCheckLevelResult = Result<TypeCheckGuard, TypeCheckFailReason>;

#[derive(Debug, Clone, Copy)]
pub struct TypeCheckGuard {
    stack_level: i32,
}

impl TypeCheckGuard {
    pub fn new() -> Self {
        Self { stack_level: 0 }
    }

    pub fn next_level(&self) -> TypeCheckLevelResult {
        let next_level = self.stack_level + 1;
        if next_level > MAX_TYPE_CHECK_LEVEL {
            return Err(TypeCheckFailReason::TypeRecursion);
        }

        Ok(Self {
            stack_level: next_level,
        })
    }
}
