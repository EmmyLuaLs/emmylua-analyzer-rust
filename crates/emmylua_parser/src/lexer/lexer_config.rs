use crate::{LuaFeatures, LuaFeaturesSet, kind::LuaLanguageLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexerConfig {
    pub language_level: LuaLanguageLevel,
    features: LuaFeaturesSet,
}

impl LexerConfig {
    pub fn new(language_level: LuaLanguageLevel) -> Self {
        LexerConfig {
            language_level,
            features: match language_level {
                LuaLanguageLevel::Lua51 => LuaFeaturesSet::features_lua51(),
                LuaLanguageLevel::Lua52 => LuaFeaturesSet::features_lua52(),
                LuaLanguageLevel::Lua53 => LuaFeaturesSet::features_lua53(),
                LuaLanguageLevel::Lua54 => LuaFeaturesSet::features_lua54(),
                LuaLanguageLevel::LuaJIT => LuaFeaturesSet::features_luajit(),
                LuaLanguageLevel::LuaJITExt => LuaFeaturesSet::features_luajit_extension(),
                LuaLanguageLevel::Lua55 => LuaFeaturesSet::features_lua55(),
            },
        }
    }

    pub fn new_with_extended_features(
        language_level: LuaLanguageLevel,
        features: LuaFeaturesSet,
    ) -> Self {
        let mut config = Self::new(language_level);
        config.features.extends_set(features);
        config
    }

    pub fn support(&self, feature: LuaFeatures) -> bool {
        self.features.support(feature)
    }
}

impl Default for LexerConfig {
    fn default() -> Self {
        LexerConfig::new(LuaLanguageLevel::Lua55)
    }
}
