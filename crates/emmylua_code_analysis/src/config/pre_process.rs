use regex::Regex;
use std::{collections::HashSet, path::PathBuf, process::Command};

use crate::config::configs::{EmmyLibraryConfig, EmmyLibraryItem};

pub struct PreProcessContext {
    workspace: PathBuf,
    luarocks_deploy_dir: String,
    placeholder_regex: Option<Regex>,
    env_var_regex: Option<Regex>,
}

impl PreProcessContext {
    pub fn new(workspace: PathBuf) -> Self {
        let luarocks_deploy_dir = get_luarocks_deploy_dir();

        Self {
            workspace,
            luarocks_deploy_dir,
            env_var_regex: match Regex::new(r"\$(\w+)") {
                Ok(re) => Some(re),
                Err(_) => {
                    log::error!(
                        "Warning: Failed to create regex for environment variable replacement"
                    );
                    None
                }
            },
            placeholder_regex: match Regex::new(r"\{([^}]+)\}") {
                Ok(re) => Some(re),
                Err(_) => {
                    log::error!(
                        "Warning: Failed to create regex for environment variable replacement"
                    );
                    None
                }
            },
        }
    }

    pub fn process_and_dedup_string<'a>(
        &self,
        iter: impl Iterator<Item = &'a String>,
    ) -> Vec<String> {
        let mut seen = HashSet::new();
        iter.map(|root| self.pre_process_path(root))
            .filter(|path| seen.insert(path.clone()))
            .collect()
    }

    pub fn process_and_dedup_library<'a>(
        &mut self,
        iter: impl Iterator<Item = &'a EmmyLibraryItem>,
    ) -> Vec<EmmyLibraryItem> {
        let mut seen = HashSet::new();
        iter.map(|root| self.pre_process_library(root))
            .filter(|path| seen.insert(path.clone()))
            .collect()
    }

    fn pre_process_path(&self, path: &str) -> String {
        let mut path = path.to_string();
        path = self.replace_env_var(&path);
        // ${workspaceFolder}  == {workspaceFolder}
        path = path.replace("$", "");
        let workspace_str = match self.workspace.to_str() {
            Some(path) => path,
            None => {
                log::error!("Warning: workspace path is not valid UTF-8");
                return path;
            }
        };

        path = self.replace_placeholders(&path, workspace_str);

        if path.starts_with('~') {
            let home_dir = match dirs::home_dir() {
                Some(path) => path,
                None => {
                    log::error!("Warning: Home directory not found");
                    return path;
                }
            };
            path = home_dir.join(&path[2..]).to_string_lossy().to_string();
        } else if path.starts_with("./") {
            path = self
                .workspace
                .join(&path[2..])
                .to_string_lossy()
                .to_string();
        } else if PathBuf::from(&path).is_absolute() {
            path = path.to_string();
        } else {
            path = self.workspace.join(&path).to_string_lossy().to_string();
        }

        path
    }

    fn pre_process_library(&mut self, item: &EmmyLibraryItem) -> EmmyLibraryItem {
        match item {
            EmmyLibraryItem::Path(p) => {
                let processed_path = self.pre_process_path(p);
                EmmyLibraryItem::Path(processed_path)
            }
            EmmyLibraryItem::Config(config) => {
                let processed_path = self.pre_process_path(&config.path);
                let old_workspace = self.workspace.clone();
                self.workspace = PathBuf::from(&processed_path);
                let processed_ignore_dir = config
                    .ignore_dir
                    .iter()
                    .map(|d| self.pre_process_path(d))
                    .collect();
                self.workspace = old_workspace;
                let processed_ignore_globs = config.ignore_globs.clone();

                EmmyLibraryItem::Config(EmmyLibraryConfig {
                    path: processed_path,
                    ignore_dir: processed_ignore_dir,
                    ignore_globs: processed_ignore_globs,
                })
            }
        }
    }

    // compact luals
    fn replace_env_var(&self, path: &str) -> String {
        let re = match &self.env_var_regex {
            Some(re) => re,
            None => return path.to_string(),
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

    fn replace_placeholders(&self, input: &str, workspace_folder: &str) -> String {
        let re = match &self.placeholder_regex {
            Some(re) => re,
            None => return input.to_string(),
        };
        re.replace_all(input, |caps: &regex::Captures| {
            let key = &caps[1];
            if key == "workspaceFolder" {
                workspace_folder.to_string()
            } else if let Some(env_name) = key.strip_prefix("env:") {
                std::env::var(env_name).unwrap_or_default()
            } else if key == "luarocks" {
                self.luarocks_deploy_dir.clone()
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
    }
}

fn get_luarocks_deploy_dir() -> String {
    Command::new("luarocks")
        .args(["config", "deploy_lua_dir"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}
