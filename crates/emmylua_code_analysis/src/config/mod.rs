mod config_loader;
mod configs;
mod flatten_config;
mod pre_process;

use std::{collections::HashMap, path::Path};

pub use config_loader::{load_configs, load_configs_raw};
pub use configs::{
    DiagnosticSeveritySetting, DocSyntax, EmmyLibraryConfig, EmmyLibraryItem, EmmyrcCodeAction,
    EmmyrcCodeLens, EmmyrcCompletion, EmmyrcDiagnostic, EmmyrcDoc, EmmyrcDocumentColor,
    EmmyrcExternalTool, EmmyrcFilenameConvention, EmmyrcHover, EmmyrcInlayHint, EmmyrcInlineValues,
    EmmyrcLuaVersion, EmmyrcReference, EmmyrcReformat, EmmyrcResource, EmmyrcRuntime,
    EmmyrcSemanticToken, EmmyrcSignature, EmmyrcStrict, EmmyrcWorkspace, EmmyrcWorkspaceModuleMap,
};
use emmylua_parser::{LuaLanguageLevel, LuaNonStdSymbolSet, ParserConfig, SpecialFunction};
use rowan::NodeCache;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::pre_process::PreProcessContext;

#[derive(Serialize, Deserialize, Debug, JsonSchema, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Emmyrc {
    #[serde(rename = "$schema")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default)]
    pub completion: EmmyrcCompletion,
    #[serde(default)]
    pub diagnostics: EmmyrcDiagnostic,
    #[serde(default)]
    pub signature: EmmyrcSignature,
    #[serde(default)]
    pub hint: EmmyrcInlayHint,
    #[serde(default)]
    pub runtime: EmmyrcRuntime,
    #[serde(default)]
    pub workspace: EmmyrcWorkspace,
    #[serde(default)]
    pub resource: EmmyrcResource,
    #[serde(default)]
    pub code_lens: EmmyrcCodeLens,
    #[serde(default)]
    pub strict: EmmyrcStrict,
    #[serde(default)]
    pub semantic_tokens: EmmyrcSemanticToken,
    #[serde(default)]
    pub references: EmmyrcReference,
    #[serde(default)]
    pub hover: EmmyrcHover,
    #[serde(default)]
    pub document_color: EmmyrcDocumentColor,
    #[serde(default)]
    pub code_action: EmmyrcCodeAction,
    #[serde(default)]
    pub inline_values: EmmyrcInlineValues,
    #[serde(default)]
    pub doc: EmmyrcDoc,
    #[serde(default)]
    pub format: EmmyrcReformat,
}

impl Emmyrc {
    pub fn get_parse_config<'cache>(
        &self,
        node_cache: &'cache mut NodeCache,
    ) -> ParserConfig<'cache> {
        let lua_language_level = self.get_language_level();
        let mut special_like = HashMap::new();
        for (name, func) in self.runtime.special.iter() {
            if let Some(func) = (*func).into() {
                special_like.insert(name.clone(), func);
            }
        }
        for name in self.runtime.require_like_function.iter() {
            special_like.insert(name.clone(), SpecialFunction::Require);
        }
        let mut non_std_symbols = LuaNonStdSymbolSet::new();
        for symbol in self.runtime.nonstandard_symbol.iter() {
            non_std_symbols.add((*symbol).into());
        }

        ParserConfig::new(
            lua_language_level,
            Some(node_cache),
            special_like,
            non_std_symbols,
            true,
        )
    }

    pub fn get_language_level(&self) -> LuaLanguageLevel {
        match self.runtime.version {
            EmmyrcLuaVersion::Lua51 => LuaLanguageLevel::Lua51,
            EmmyrcLuaVersion::Lua52 => LuaLanguageLevel::Lua52,
            EmmyrcLuaVersion::Lua53 => LuaLanguageLevel::Lua53,
            EmmyrcLuaVersion::Lua54 => LuaLanguageLevel::Lua54,
            EmmyrcLuaVersion::LuaJIT => LuaLanguageLevel::LuaJIT,
            EmmyrcLuaVersion::Lua55 => LuaLanguageLevel::Lua55,
            EmmyrcLuaVersion::LuaLatest => LuaLanguageLevel::Lua55,
        }
    }

    pub fn pre_process_emmyrc(&mut self, workspace_root: &Path) {
        let mut context = PreProcessContext::new(workspace_root.to_path_buf());

        self.workspace.workspace_roots =
            context.process_and_dedup_string(self.workspace.workspace_roots.iter());

        self.workspace.library = context.process_and_dedup_library(self.workspace.library.iter());

        self.workspace.package_dirs =
            context.process_and_dedup_string(self.workspace.package_dirs.iter());

        self.workspace.ignore_dir =
            context.process_and_dedup_string(self.workspace.ignore_dir.iter());

        self.resource.paths = context.process_and_dedup_string(self.resource.paths.iter());
    }
}
