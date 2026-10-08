mod config_loader;
mod configs;
mod flatten_config;
mod lua_loader;
mod pre_process;

use std::{collections::HashMap, path::Path};

pub use config_loader::{load_configs, load_configs_raw};
pub use configs::{
    DiagnosticSeveritySetting, DocSyntax, EmmyLibraryConfig, EmmyLibraryItem, EmmyrcCodeAction,
    EmmyrcCodeLens, EmmyrcCompletion, EmmyrcDiagnostic, EmmyrcDoc, EmmyrcDocumentColor,
    EmmyrcExternalTool, EmmyrcFilenameConvention, EmmyrcHover, EmmyrcInlayHint, EmmyrcInlineValues,
    EmmyrcLuaVersion, EmmyrcReference, EmmyrcReformat, EmmyrcResource, EmmyrcRuntime,
    EmmyrcSemanticToken, EmmyrcSignature, EmmyrcStrict, EmmyrcWorkspace, EmmyrcWorkspaceModuleMap,
    EmmyrcWorkspacePathConfig, EmmyrcWorkspacePathItem,
};
use emmylua_parser::{LuaFeaturesSet, LuaLanguageLevel, ParserConfig, SpecialFunction};
use rowan::NodeCache;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

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
        let mut non_std_symbols = LuaFeaturesSet::default();
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
        self.runtime.version.get_language_level()
    }

    pub(crate) fn module_patterns(&self) -> Vec<SmolStr> {
        let mut extensions: Vec<SmolStr> = self
            .runtime
            .extensions
            .iter()
            .map(|ext| {
                SmolStr::new(
                    ext.strip_prefix(".")
                        .or_else(|| ext.strip_prefix("*."))
                        .unwrap_or(ext),
                )
            })
            .collect();
        if !extensions.iter().any(|ext| ext == "lua") {
            extensions.push(SmolStr::new("lua"));
        }
        let mut patterns: Vec<SmolStr> = extensions
            .iter()
            .map(|ext| SmolStr::new(format!("?.{}", ext)))
            .collect();
        if self.runtime.require_pattern.is_empty() {
            for ext in &extensions {
                patterns.push(SmolStr::new(format!("?/init.{}", ext)));
            }
        } else {
            patterns.extend(self.runtime.require_pattern.iter().map(SmolStr::new));
        }
        patterns
    }

    pub(crate) fn module_replace(&self) -> Vec<(SmolStr, SmolStr)> {
        self.workspace
            .module_map
            .iter()
            .map(|module| (SmolStr::new(&module.pattern), SmolStr::new(&module.replace)))
            .collect()
    }

    pub(crate) fn known_doc_tags(&self) -> Vec<SmolStr> {
        self.doc
            .known_tags
            .iter()
            .map(|tag| SmolStr::new(tag.as_str()))
            .collect()
    }

    pub fn pre_process_emmyrc(&mut self, workspace_root: &Path) {
        let mut context = PreProcessContext::new(workspace_root.to_path_buf());

        self.workspace.workspace_roots =
            context.process_and_dedup_string(self.workspace.workspace_roots.iter());

        self.workspace.library =
            context.process_and_dedup_workspace_path_items(self.workspace.library.iter());

        self.workspace.packages =
            context.process_and_dedup_workspace_path_items(self.workspace.packages.iter());

        self.workspace.ignore_dir =
            context.process_and_dedup_string(self.workspace.ignore_dir.iter());

        self.resource.paths = context.process_and_dedup_string(self.resource.paths.iter());
    }
}
