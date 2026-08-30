use std::time::Duration;

use log::info;
use serde_json::Value;

use crate::{context::ServerContextSnapshot, util::time_cancel_token};
use emmylua_code_analysis::file_path_to_uri;

use super::ClientConfig;

pub async fn get_client_config_default(
    context: &ServerContextSnapshot,
    config: &mut ClientConfig,
    scopes: Option<&[&str]>,
) -> Option<()> {
    let workspace_folders = context
        .workspace_manager()
        .lock()
        .await
        .workspace_folders
        .clone();
    let main_workspace_folder = workspace_folders.first();
    let client = context.client();
    let scope_uri = main_workspace_folder.and_then(|p| file_path_to_uri(&p.root));

    let mut configs = Vec::new();
    let mut used_scope = None;
    for scope in scopes.unwrap_or(&["emmylua"]) {
        let params = lsp_types::ConfigurationParams {
            items: vec![lsp_types::ConfigurationItem {
                scope_uri: scope_uri.clone(),
                section: Some(scope.to_string()),
            }],
        };
        log::info!("fetching client config for scope {scope:?}");
        let cancel_token = time_cancel_token(Duration::from_secs(5));
        let fetched_configs: Vec<_> = match client
            .get_configuration::<Value>(params, cancel_token)
            .await
        {
            Some(configs) => configs,
            None => {
                log::warn!("failed to fetch client config for scope {scope:?}");
                continue;
            }
        };
        let fetched_configs: Vec<_> = fetched_configs
            .into_iter()
            .filter(|config| !config.is_null())
            .collect();
        if !fetched_configs.is_empty() {
            info!("found client config in scope {scope:?}");
            configs = fetched_configs;
            used_scope = Some(scope.to_string());
        }
    }

    if let Some(used_scope) = used_scope {
        info!(
            "using client config from scope {used_scope:?}: {}",
            serde_json::to_string_pretty(&configs)
                .as_deref()
                .unwrap_or("<failed to serialize json>")
        );
    } else {
        info!("no client config found");
    }

    for config in &mut configs {
        // VSCode always sends default values for all options, even those that weren't
        // explicitly configured by user. This results in `null`s being sent for
        // every option. Naturally, serde chokes on these nulls when applying partial
        // configuration.
        //
        // Because of this, we have to ignore them here.
        skip_nulls(config);
    }

    config.partial_emmyrcs = Some(configs);

    Some(())
}

fn skip_nulls(v: &mut Value) {
    if let Value::Object(obj) = v {
        obj.retain(|_, v| !v.is_null());
        for (_, v) in obj {
            skip_nulls(v);
        }
    }
}
