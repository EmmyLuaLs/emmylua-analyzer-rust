mod config_loader;
mod configs;
mod flatten_config;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub use crate::config::configs::{EmmyrcExternalTool, EmmyrcReformat};
pub use config_loader::{load_configs, load_configs_raw};
pub use configs::{DocSyntax, EmmyrcFilenameConvention, EmmyrcLuaVersion};
use configs::{
    EmmyrcCodeAction, EmmyrcCodeLens, EmmyrcCompletion, EmmyrcDiagnostic, EmmyrcDoc,
    EmmyrcDocumentColor, EmmyrcHover, EmmyrcInlayHint, EmmyrcInlineValues, EmmyrcReference,
    EmmyrcResource, EmmyrcRuntime, EmmyrcSemanticToken, EmmyrcSignature, EmmyrcStrict,
    EmmyrcWorkspace,
};
use emmylua_parser::{LuaLanguageLevel, LuaNonStdSymbolSet, ParserConfig, SpecialFunction};
use regex::Regex;
use rowan::NodeCache;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Emmyrc {
    #[serde(rename = "$schema")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Configuration for code completion features.
    #[serde(default)]
    pub completion: EmmyrcCompletion,

    /// Configuration for diagnostics and error detection.
    #[serde(default)]
    pub diagnostics: EmmyrcDiagnostic,

    /// Configuration for function signature help.
    #[serde(default)]
    pub signature: EmmyrcSignature,

    /// Configuration for inlay hints in the editor.
    #[serde(default)]
    pub hint: EmmyrcInlayHint,

    /// Configuration for Lua runtime.
    #[serde(default)]
    pub runtime: EmmyrcRuntime,

    /// Configuration for workspace management.
    #[serde(default)]
    pub workspace: EmmyrcWorkspace,

    /// Configuration for resource file management.
    #[serde(default)]
    pub resource: EmmyrcResource,

    /// Configuration for code lens features.
    #[serde(default)]
    pub code_lens: EmmyrcCodeLens,

    /// Configuration for strict type checks.
    #[serde(default)]
    pub strict: EmmyrcStrict,

    /// Configuration for semantic token highlighting.
    #[serde(default)]
    pub semantic_tokens: EmmyrcSemanticToken,

    /// Configuration for reference lookup features.
    #[serde(default)]
    pub references: EmmyrcReference,

    /// Configuration for hover information.
    #[serde(default)]
    pub hover: EmmyrcHover,

    /// Configuration for document color features.
    #[serde(default)]
    pub document_color: EmmyrcDocumentColor,

    /// Configuration for code actions and quick fixes.
    #[serde(default)]
    pub code_action: EmmyrcCodeAction,

    /// Configuration for inline value display.
    #[serde(default)]
    pub inline_values: EmmyrcInlineValues,

    /// Configuration for documentation parsing.
    #[serde(default)]
    pub doc: EmmyrcDoc,

    /// Configuration for code formatting.
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
            if let Some(func) = func.clone().into() {
                special_like.insert(name.clone(), func);
            }
        }
        for name in self.runtime.require_like_function.iter() {
            special_like.insert(name.clone(), SpecialFunction::Require);
        }
        let mut non_std_symbols = LuaNonStdSymbolSet::new();
        for symbol in self.runtime.nonstandard_symbol.iter() {
            non_std_symbols.add(symbol.clone().into());
        }

        ParserConfig::new(
            lua_language_level,
            Some(node_cache),
            special_like,
            non_std_symbols,
        )
    }

    pub fn get_language_level(&self) -> LuaLanguageLevel {
        match self.runtime.version {
            EmmyrcLuaVersion::Lua51 => LuaLanguageLevel::Lua51,
            EmmyrcLuaVersion::Lua52 => LuaLanguageLevel::Lua52,
            EmmyrcLuaVersion::Lua53 => LuaLanguageLevel::Lua53,
            EmmyrcLuaVersion::Lua54 => LuaLanguageLevel::Lua54,
            EmmyrcLuaVersion::LuaJIT => LuaLanguageLevel::LuaJIT,
            // wait lua5.5 release
            EmmyrcLuaVersion::LuaLatest => LuaLanguageLevel::Lua54,
            EmmyrcLuaVersion::Lua55 => LuaLanguageLevel::Lua55,
        }
    }

    pub fn pre_process_emmyrc(&mut self, workspace_root: &Path) {
        fn process_and_dedup<'a>(
            iter: impl Iterator<Item = &'a String>,
            workspace_root: &Path,
        ) -> Vec<String> {
            let mut seen = HashSet::new();
            iter.map(|root| pre_process_path(root, workspace_root))
                .filter(|path| seen.insert(path.clone()))
                .collect()
        }
        self.workspace.workspace_roots =
            process_and_dedup(self.workspace.workspace_roots.iter(), workspace_root);

        self.workspace.library = process_and_dedup(self.workspace.library.iter(), workspace_root);

        self.workspace.ignore_dir =
            process_and_dedup(self.workspace.ignore_dir.iter(), workspace_root);

        self.resource.paths = process_and_dedup(self.resource.paths.iter(), workspace_root);
    }
}

fn pre_process_path(path: &str, workspace: &Path) -> String {
    let mut path = path.to_string();
    path = replace_env_var(&path);
    // ${workspaceFolder}  == {workspaceFolder}
    path = path.replace("$", "");
    let workspace_str = match workspace.to_str() {
        Some(path) => path,
        None => {
            log::error!("Warning: workspace path is not valid UTF-8");
            return path;
        }
    };

    path = replace_placeholders(&path, workspace_str);

    if path.starts_with('~') {
        let home_dir = match dirs::home_dir() {
            Some(path) => path,
            None => {
                log::error!("Warning: Home directory not found");
                return path;
            }
        };
        path = home_dir.join(&path[1..]).to_string_lossy().to_string();
    } else if path.starts_with("./") {
        path = workspace.join(&path[2..]).to_string_lossy().to_string();
    } else if PathBuf::from(&path).is_absolute() {
        path = path.to_string();
    } else {
        path = workspace.join(&path).to_string_lossy().to_string();
    }

    path
}

// compact luals
fn replace_env_var(path: &str) -> String {
    let re = match Regex::new(r"\$(\w+)") {
        Ok(re) => re,
        Err(_) => {
            log::error!("Warning: Failed to create regex for environment variable replacement");
            return path.to_string();
        }
    };
    re.replace_all(path, |caps: &regex::Captures| {
        let key = &caps[1];
        std::env::var(key).unwrap_or_else(|_| {
            log::error!("Warning: Environment variable {} is not set", key);
            String::new()
        })
    })
    .to_string()
}

fn replace_placeholders(input: &str, workspace_folder: &str) -> String {
    let re = match Regex::new(r"\{([^}]+)\}") {
        Ok(re) => re,
        Err(_) => {
            log::error!("Warning: Failed to create regex for placeholder replacement");
            return input.to_string();
        }
    };
    re.replace_all(input, |caps: &regex::Captures| {
        let key = &caps[1];
        if key == "workspaceFolder" {
            workspace_folder.to_string()
        } else if let Some(env_name) = key.strip_prefix("env:") {
            std::env::var(env_name).unwrap_or_default()
        } else {
            caps[0].to_string()
        }
    })
    .to_string()
}
