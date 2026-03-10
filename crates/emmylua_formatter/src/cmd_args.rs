use std::{fs, path::PathBuf};

use clap::{ArgGroup, Parser};

use crate::styles::{LuaCodeStyle, LuaIndent};

#[derive(Debug, Clone, Parser)]
#[command(
    name = "emmylua_format",
    version,
    about = "Format Lua source code using EmmyLua code style rules",
    disable_help_subcommand = true
)]
#[command(group(
	ArgGroup::new("indent_choice")
		.args(["tab", "spaces"])
		.multiple(false)
))]
pub struct CliArgs {
    /// Input paths to format (files only). If omitted, reads from stdin.
    #[arg(value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
    pub paths: Vec<PathBuf>,

    /// Read source from stdin instead of files
    #[arg(long)]
    pub stdin: bool,

    /// Write formatted result back to the file(s)
    #[arg(long)]
    pub write: bool,

    /// Check if files would be reformatted. Exit with code 1 if any would change.
    #[arg(long)]
    pub check: bool,

    /// Print paths of files that would be reformatted
    #[arg(long, alias = "list-different")]
    pub list_different: bool,

    /// Write output to a specific file (only with a single input or stdin)
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub output: Option<PathBuf>,

    /// Load style config from a file (json/yml/yaml)
    #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub config: Option<PathBuf>,

    /// Use tabs for indentation
    #[arg(long)]
    pub tab: bool,

    /// Use N spaces for indentation (mutually exclusive with --tab)
    #[arg(long, value_name = "N")]
    pub spaces: Option<usize>,

    /// Set maximum line width
    #[arg(long, value_name = "N")]
    pub max_line_width: Option<usize>,
}

pub fn resolve_style(args: &CliArgs) -> Result<LuaCodeStyle, String> {
    let mut style = if let Some(cfg) = &args.config {
        let content = fs::read_to_string(cfg)
            .map_err(|e| format!("读取配置失败: {}: {e}", cfg.to_string_lossy()))?;
        let ext = cfg
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "json" => serde_json::from_str::<LuaCodeStyle>(&content)
                .map_err(|e| format!("解析 JSON 配置失败: {e}"))?,
            "yml" | "yaml" => serde_yml::from_str::<LuaCodeStyle>(&content)
                .map_err(|e| format!("解析 YAML 配置失败: {e}"))?,
            _ => {
                // Unknown extension, try JSON first then YAML
                match serde_json::from_str::<LuaCodeStyle>(&content) {
                    Ok(v) => v,
                    Err(_) => serde_yml::from_str::<LuaCodeStyle>(&content)
                        .map_err(|e| format!("未知扩展名，按 JSON/YAML 解析均失败: {e}"))?,
                }
            }
        }
    } else {
        LuaCodeStyle::default()
    };

    // Indent overrides
    match (args.tab, args.spaces) {
        (true, Some(_)) => return Err("--tab 与 --spaces 不能同时使用".into()),
        (true, None) => style.indent = LuaIndent::Tab,
        (false, Some(n)) => style.indent = LuaIndent::Space(n),
        _ => {}
    }

    if let Some(w) = args.max_line_width {
        style.max_line_width = w;
    }

    Ok(style)
}
