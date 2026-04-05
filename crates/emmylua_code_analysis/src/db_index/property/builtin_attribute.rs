use crate::{LuaType, LuaTypeDeclId};

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
            return_self: self.get_bool_param("return_self").unwrap_or(true),
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
    /// 是否返回`self`
    pub return_self: bool,
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

    use super::{LuaAttributeUse, LuaFieldAccessorConvention, LuaLspOptimizationCode};
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
        assert!(constructor.return_self);
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
