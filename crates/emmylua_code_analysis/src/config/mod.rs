mod config_loader;
mod configs;
mod flatten_config;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub use config_loader::load_configs;
pub use configs::EmmyrcFilenameConvention;
pub use configs::EmmyrcLuaVersion;
use configs::{EmmyrcCodeAction, EmmyrcDocumentColor};
use configs::{
    EmmyrcCodeLen, EmmyrcCompletion, EmmyrcDiagnostic, EmmyrcHover, EmmyrcInlayHint,
    EmmyrcInlineValues, EmmyrcReference, EmmyrcResource, EmmyrcRuntime, EmmyrcSemanticToken,
    EmmyrcSignature, EmmyrcStrict, EmmyrcWorkspace,
};
use emmylua_parser::{LuaLanguageLevel, ParserConfig, SpecialFunction};
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
    pub code_lens: EmmyrcCodeLen,
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
}

impl Emmyrc {
    pub fn get_parse_config<'cache>(
        &self,
        node_cache: &'cache mut NodeCache,
    ) -> ParserConfig<'cache> {
        let level = &self.runtime.version;

        let lua_language_level = match level {
            EmmyrcLuaVersion::Lua51 => LuaLanguageLevel::Lua51,
            EmmyrcLuaVersion::Lua52 => LuaLanguageLevel::Lua52,
            EmmyrcLuaVersion::Lua53 => LuaLanguageLevel::Lua53,
            EmmyrcLuaVersion::Lua54 => LuaLanguageLevel::Lua54,
            EmmyrcLuaVersion::LuaJIT => LuaLanguageLevel::LuaJIT,
            EmmyrcLuaVersion::LuaLatest => LuaLanguageLevel::Lua54,
            EmmyrcLuaVersion::Lua55 => LuaLanguageLevel::Lua55,
        };

        let mut special_like = HashMap::new();
        for name in self.runtime.require_like_function.iter() {
            special_like.insert(name.clone(), SpecialFunction::Require);
        }
        ParserConfig::new(lua_language_level, Some(node_cache), special_like)
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

    fn canonicalize_path(path_buf: PathBuf) -> String {
        let new_path_buf = path_buf.canonicalize().unwrap_or(path_buf);
        new_path_buf.to_string_lossy().to_string()
    }

    if path.starts_with('~') {
        let home_dir = match dirs::home_dir() {
            Some(path) => path,
            None => {
                log::error!("Warning: Home directory not found");
                return path;
            }
        };
        path = canonicalize_path(home_dir.join(&path[1..]));
    } else if path.starts_with("./") {
        path = canonicalize_path(workspace.join(&path[2..]));
    } else if PathBuf::from(&path).is_absolute() {
        path = path.to_string();
    } else {
        path = canonicalize_path(workspace.join(&path));
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
