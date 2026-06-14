use std::sync::Arc;

use crate::{
    DbIndex, LuaFunctionType, LuaOperatorMetaMethod, LuaType, LuaTypeDeclId, callable_accepts_args,
    is_sub_type_of,
};

const ATTRIBUTE_BASE_TYPE_NAME: &str = "Attribute";

pub fn is_attribute_class(db: &DbIndex, type_id: &LuaTypeDeclId) -> bool {
    let Some(type_decl) = db.get_type_index().get_type_decl(type_id) else {
        return false;
    };
    if !type_decl.is_class() {
        return false;
    }

    let attribute_type_id = LuaTypeDeclId::global(ATTRIBUTE_BASE_TYPE_NAME);
    is_sub_type_of(db, type_id, &attribute_type_id)
}

pub fn get_attribute_constructor_params(
    db: &DbIndex,
    type_id: &LuaTypeDeclId,
    arg_types: &[LuaType],
) -> Vec<(String, Option<LuaType>)> {
    select_attribute_constructor_func(db, type_id, arg_types)
        .map(|func| func.get_params().to_vec())
        .unwrap_or_default()
}

fn select_attribute_constructor_func(
    db: &DbIndex,
    type_id: &LuaTypeDeclId,
    arg_types: &[LuaType],
) -> Option<Arc<LuaFunctionType>> {
    let arg_count = arg_types.len();
    let operator_ids = db
        .get_operator_index()
        .get_operators(&type_id.clone().into(), LuaOperatorMetaMethod::Call)?;

    let mut fallback = None;
    let mut count_fallback = None;
    let only_candidate = operator_ids.len() == 1;
    for operator_id in operator_ids {
        let Some(operator) = db.get_operator_index().get_operator(operator_id) else {
            continue;
        };
        let LuaType::DocFunction(func) = operator.get_operator_func(db) else {
            continue;
        };

        let params = func.get_params();
        fallback.get_or_insert_with(|| Arc::clone(&func));
        if !attribute_params_accept_arg_count(&params, arg_count) {
            continue;
        }

        count_fallback.get_or_insert_with(|| Arc::clone(&func));
        if only_candidate || callable_accepts_args(db, &func, arg_types, false, Some(arg_count)) {
            return Some(func);
        }
    }

    count_fallback.or(fallback)
}

fn attribute_params_accept_arg_count(
    def_params: &[(String, Option<LuaType>)],
    arg_count: usize,
) -> bool {
    let required_count = def_params
        .iter()
        .take_while(|(name, typ)| name != "..." && !typ.as_ref().is_some_and(LuaType::is_variadic))
        .filter(|(_, typ)| !typ.as_ref().is_some_and(LuaType::is_optional))
        .count();

    let allows_more = def_params
        .last()
        .is_some_and(|(name, typ)| name == "..." || typ.as_ref().is_some_and(LuaType::is_variadic));

    arg_count >= required_count && (allows_more || arg_count <= def_params.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuaBuiltinAttributeKind {
    Deprecated,
    LspOptimization,
    IndexAlias,
    Constructor,
    FieldAccessor,
}

impl LuaBuiltinAttributeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deprecated => "deprecated",
            Self::LspOptimization => "lsp_optimization",
            Self::IndexAlias => "index_alias",
            Self::Constructor => "constructor",
            Self::FieldAccessor => "field_accessor",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "deprecated" => Some(Self::Deprecated),
            "lsp_optimization" => Some(Self::LspOptimization),
            "index_alias" => Some(Self::IndexAlias),
            "constructor" => Some(Self::Constructor),
            "field_accessor" => Some(Self::FieldAccessor),
            _ => None,
        }
    }
}

pub trait LuaAttributeCollectionExt {
    fn find_attribute_use(&self, id: &str) -> Option<&LuaAttributeUse>;

    fn find_builtin_attribute(&self, kind: LuaBuiltinAttributeKind) -> Option<&LuaAttributeUse> {
        self.find_attribute_use(kind.as_str())
    }
}

impl LuaAttributeCollectionExt for [LuaAttributeUse] {
    fn find_attribute_use(&self, id: &str) -> Option<&LuaAttributeUse> {
        self.iter()
            .find(|attribute_use| attribute_use.id_name() == id)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LuaAttributeUse {
    pub id: LuaTypeDeclId,
    pub args: Vec<(String, Option<LuaType>)>,
}

impl LuaAttributeUse {
    pub fn new(id: LuaTypeDeclId, args: Vec<(String, Option<LuaType>)>) -> Self {
        Self { id, args }
    }

    pub fn id_name(&self) -> &str {
        self.id.get_name()
    }

    pub fn get_param_by_name(&self, name: &str) -> Option<&LuaType> {
        self.args
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, typ)| typ.as_ref())
    }

    pub fn builtin_kind(&self) -> Option<LuaBuiltinAttributeKind> {
        LuaBuiltinAttributeKind::from_name(self.id_name())
    }

    pub fn is_builtin(&self, kind: LuaBuiltinAttributeKind) -> bool {
        self.builtin_kind() == Some(kind)
    }

    pub fn get_string_param(&self, name: &str) -> Option<&str> {
        match self.get_param_by_name(name) {
            Some(LuaType::DocStringConst(value)) => Some(value.as_ref()),
            _ => None,
        }
    }

    pub fn get_bool_param(&self, name: &str) -> Option<bool> {
        match self.get_param_by_name(name) {
            Some(LuaType::DocBooleanConst(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn as_deprecated(&self) -> Option<LuaDeprecatedAttribute<'_>> {
        if !self.is_builtin(LuaBuiltinAttributeKind::Deprecated) {
            return None;
        }

        Some(LuaDeprecatedAttribute {
            message: self.get_string_param("message"),
        })
    }

    pub fn as_lsp_optimization(&self) -> Option<LuaLspOptimizationAttribute> {
        if !self.is_builtin(LuaBuiltinAttributeKind::LspOptimization) {
            return None;
        }

        let code = match self.get_string_param("code")? {
            "check_table_field" => LuaLspOptimizationCode::CheckTableField,
            "delayed_definition" => LuaLspOptimizationCode::DelayedDefinition,
            _ => return None,
        };

        Some(LuaLspOptimizationAttribute { code })
    }

    pub fn as_index_alias(&self) -> Option<LuaIndexAliasAttribute<'_>> {
        if !self.is_builtin(LuaBuiltinAttributeKind::IndexAlias) {
            return None;
        }

        Some(LuaIndexAliasAttribute {
            name: self.get_string_param("name")?,
        })
    }

    pub fn as_constructor(&self) -> Option<LuaConstructorAttribute<'_>> {
        if !self.is_builtin(LuaBuiltinAttributeKind::Constructor) {
            return None;
        }

        Some(LuaConstructorAttribute {
            name: self.get_string_param("name")?,
            root_class: self.get_string_param("root_class"),
            strip_self: self.get_bool_param("strip_self").unwrap_or(true),
            return_mode: match self.get_param_by_name("return_mode") {
                Some(LuaType::DocStringConst(value)) => {
                    LuaConstructorReturnMode::from_name(value.as_ref())?
                }
                _ => LuaConstructorReturnMode::Default,
            },
        })
    }

    pub fn as_field_accessor(&self) -> Option<LuaFieldAccessorAttribute<'_>> {
        if !self.is_builtin(LuaBuiltinAttributeKind::FieldAccessor) {
            return None;
        }

        let convention = self
            .get_string_param("convention")
            .and_then(LuaFieldAccessorConvention::from_name)
            .unwrap_or(LuaFieldAccessorConvention::CamelCase);

        Some(LuaFieldAccessorAttribute {
            convention,
            getter: self.get_string_param("getter"),
            setter: self.get_string_param("setter"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuaDeprecatedAttribute<'a> {
    pub message: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuaLspOptimizationCode {
    CheckTableField,
    DelayedDefinition,
}

impl LuaLspOptimizationCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CheckTableField => "check_table_field",
            Self::DelayedDefinition => "delayed_definition",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuaLspOptimizationAttribute {
    pub code: LuaLspOptimizationCode,
}

impl LuaLspOptimizationAttribute {
    pub fn is_check_table_field(self) -> bool {
        self.code == LuaLspOptimizationCode::CheckTableField
    }

    pub fn is_delayed_definition(self) -> bool {
        self.code == LuaLspOptimizationCode::DelayedDefinition
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuaIndexAliasAttribute<'a> {
    pub name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuaConstructorAttribute<'a> {
    /// 构造函数名
    pub name: &'a str,
    /// 根类名
    pub root_class: Option<&'a str>,
    /// 是否移除`self`参数
    pub strip_self: bool,
    /// 构造函数返回策略
    pub return_mode: LuaConstructorReturnMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuaConstructorReturnMode {
    SelfType,
    Doc,
    Default,
}

impl LuaConstructorReturnMode {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "self" => Some(Self::SelfType),
            "doc" => Some(Self::Doc),
            "default" => Some(Self::Default),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuaFieldAccessorConvention {
    CamelCase,
    PascalCase,
    SnakeCase,
}

impl LuaFieldAccessorConvention {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "camelCase" => Some(Self::CamelCase),
            "PascalCase" => Some(Self::PascalCase),
            "snake_case" => Some(Self::SnakeCase),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LuaFieldAccessorAttribute<'a> {
    pub convention: LuaFieldAccessorConvention,
    pub getter: Option<&'a str>,
    pub setter: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use smol_str::SmolStr;

    use super::{
        LuaAttributeUse, LuaConstructorReturnMode, LuaFieldAccessorConvention,
        LuaLspOptimizationCode,
    };
    use crate::{LuaType, LuaTypeDeclId};

    fn doc_string(value: &str) -> LuaType {
        LuaType::DocStringConst(SmolStr::new(value).into())
    }

    #[test]
    fn constructor_attribute_uses_builtin_defaults() {
        let attribute = LuaAttributeUse::new(
            LuaTypeDeclId::global("constructor"),
            vec![("name".into(), Some(doc_string("__init")))],
        );

        let constructor = attribute.as_constructor().unwrap();
        assert_eq!(constructor.name, "__init");
        assert_eq!(constructor.root_class, None);
        assert!(constructor.strip_self);
        assert_eq!(constructor.return_mode, LuaConstructorReturnMode::Default);
    }

    #[test]
    fn constructor_attribute_supports_string_return_mode() {
        let attribute = LuaAttributeUse::new(
            LuaTypeDeclId::global("constructor"),
            vec![
                ("name".into(), Some(doc_string("__init"))),
                ("return_mode".into(), Some(doc_string("doc"))),
            ],
        );

        let constructor = attribute.as_constructor().unwrap();
        assert_eq!(constructor.return_mode, LuaConstructorReturnMode::Doc);
    }

    #[test]
    fn field_accessor_defaults_to_camel_case() {
        let attribute = LuaAttributeUse::new(LuaTypeDeclId::global("field_accessor"), Vec::new());

        let field_accessor = attribute.as_field_accessor().unwrap();
        assert_eq!(
            field_accessor.convention,
            LuaFieldAccessorConvention::CamelCase
        );
        assert_eq!(field_accessor.getter, None);
        assert_eq!(field_accessor.setter, None);
    }

    #[test]
    fn lsp_optimization_parses_known_codes() {
        let attribute = LuaAttributeUse::new(
            LuaTypeDeclId::global("lsp_optimization"),
            vec![("code".into(), Some(doc_string("delayed_definition")))],
        );

        let lsp_optimization = attribute.as_lsp_optimization().unwrap();
        assert_eq!(
            lsp_optimization.code,
            LuaLspOptimizationCode::DelayedDefinition
        );
    }
}
