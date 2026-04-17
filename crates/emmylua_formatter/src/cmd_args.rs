use std::path::{Path, PathBuf};

use clap::{ArgGroup, Parser, ValueEnum};
use emmylua_parser::LuaLanguageLevel;

use crate::{
    FileCollectorOptions, IndentKind, LuaSyntaxLevel, ResolvedConfig, resolve_config_for_path,
};

#[derive(Debug, Clone, Parser)]
#[command(
    name = "luafmt",
    version,
    about = "Format Lua source code with structured EmmyLua formatter settings",
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

    /// Colorize --check diff output
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Diff rendering style for --check output
    #[arg(long, value_enum, default_value_t = DiffStyle::Marker)]
    pub diff_style: DiffStyle,

    /// Write output to a specific file (only with a single input or stdin)
    #[arg(short, long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub output: Option<PathBuf>,

    /// Load style config from a file (toml/json/yml/yaml)
    #[arg(long, value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
    pub config: Option<PathBuf>,

    /// Print the default configuration as TOML and exit
    #[arg(long)]
    pub dump_default_config: bool,

    /// Use tabs for indentation
    #[arg(long)]
    pub tab: bool,

    /// Use N spaces for indentation (mutually exclusive with --tab)
    #[arg(long, value_name = "N")]
    pub spaces: Option<usize>,

    /// Set maximum line width
    #[arg(long, value_name = "N")]
    pub max_line_width: Option<usize>,

    /// Recurse into directories to find Lua files
    #[arg(long, default_value_t = true)]
    pub recursive: bool,

    /// Include hidden files and directories
    #[arg(long)]
    pub include_hidden: bool,

    /// Follow symlinks while walking directories
    #[arg(long)]
    pub follow_symlinks: bool,

    /// Disable .luafmtignore support
    #[arg(long)]
    pub no_ignore: bool,

    /// Include files matching an additional glob pattern
    #[arg(long, value_name = "GLOB")]
    pub include: Vec<String>,

    /// Exclude files matching a glob pattern
    #[arg(long, value_name = "GLOB")]
    pub exclude: Vec<String>,

    #[arg(long, value_enum)]
    pub level: Option<LanguageLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum DiffStyle {
    #[default]
    Marker,
    Git,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum LanguageLevel {
    Lua51,
    Lua52,
    Lua53,
    Lua54,
    #[default]
    Lua55,
    LuaJIT,
}

impl From<LanguageLevel> for LuaLanguageLevel {
    fn from(level: LanguageLevel) -> Self {
        match level {
            LanguageLevel::Lua51 => LuaLanguageLevel::Lua51,
            LanguageLevel::Lua52 => LuaLanguageLevel::Lua52,
            LanguageLevel::Lua53 => LuaLanguageLevel::Lua53,
            LanguageLevel::Lua54 => LuaLanguageLevel::Lua54,
            LanguageLevel::Lua55 => LuaLanguageLevel::Lua55,
            LanguageLevel::LuaJIT => LuaLanguageLevel::LuaJIT,
        }
    }
}

impl From<LanguageLevel> for LuaSyntaxLevel {
    fn from(level: LanguageLevel) -> Self {
        match level {
            LanguageLevel::Lua51 => LuaSyntaxLevel::Lua51,
            LanguageLevel::Lua52 => LuaSyntaxLevel::Lua52,
            LanguageLevel::Lua53 => LuaSyntaxLevel::Lua53,
            LanguageLevel::Lua54 => LuaSyntaxLevel::Lua54,
            LanguageLevel::Lua55 => LuaSyntaxLevel::Lua55,
            LanguageLevel::LuaJIT => LuaSyntaxLevel::LuaJIT,
        }
    }
}

fn apply_style_overrides(args: &CliArgs, resolved: &mut ResolvedConfig) -> Result<(), String> {
    if let Some(level) = args.level {
        resolved.config.syntax.level = level.into();
    }

    // Indent overrides
    match (args.tab, args.spaces) {
        (true, Some(_)) => return Err("--tab and --spaces are mutually exclusive".into()),
        (true, None) => resolved.config.indent.kind = IndentKind::Tab,
        (false, Some(n)) => {
            resolved.config.indent.kind = IndentKind::Space;
            resolved.config.indent.width = n;
        }
        _ => {}
    }

    if let Some(w) = args.max_line_width {
        resolved.config.layout.max_line_width = w;
    }

    Ok(())
}

pub fn resolve_style(args: &CliArgs, source_path: Option<&Path>) -> Result<ResolvedConfig, String> {
    let mut resolved = resolve_config_for_path(source_path, args.config.as_deref())
        .map_err(|err| err.to_string())?;
    apply_style_overrides(args, &mut resolved)?;
    Ok(resolved)
}

pub fn build_file_collector_options(args: &CliArgs) -> FileCollectorOptions {
    FileCollectorOptions {
        recursive: args.recursive,
        include_hidden: args.include_hidden,
        follow_symlinks: args.follow_symlinks,
        respect_ignore_files: !args.no_ignore,
        include: args.include.clone(),
        exclude: args.exclude.clone(),
    }
}
