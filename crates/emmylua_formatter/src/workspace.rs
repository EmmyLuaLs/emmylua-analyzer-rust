use std::{
    collections::BTreeSet,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use emmylua_parser::LuaLanguageLevel;
use glob::Pattern;
use toml_edit::{de::from_str as from_toml_str, ser::to_string_pretty as to_toml_string};
use walkdir::{DirEntry, WalkDir};

use crate::{LuaFormatConfig, SourceText, reformat_lua_code};

const CONFIG_FILE_NAMES: [&str; 2] = [".luafmt.toml", "luafmt.toml"];
const IGNORE_FILE_NAME: &str = ".luafmtignore";
const DEFAULT_IGNORED_DIRS: [&str; 5] = [".git", ".hg", ".svn", "node_modules", "target"];

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub config: LuaFormatConfig,
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOutput {
    pub formatted: String,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatCheckResult {
    pub formatted: String,
    pub changed: bool,
    pub changed_line_ranges: Vec<ChangedLineRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedLineRange {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatPathResult {
    pub path: PathBuf,
    pub output: FormatOutput,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatCheckPathResult {
    pub path: PathBuf,
    pub output: FormatCheckResult,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FileCollectorOptions {
    pub recursive: bool,
    pub include_hidden: bool,
    pub follow_symlinks: bool,
    pub respect_ignore_files: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Default for FileCollectorOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            include_hidden: false,
            follow_symlinks: false,
            respect_ignore_files: true,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum FormatterError {
    Io(io::Error),
    SyntaxError(String),
    ConfigRead {
        path: PathBuf,
        source: io::Error,
    },
    ConfigParse {
        path: Option<PathBuf>,
        message: String,
    },
    GlobPattern {
        pattern: String,
        message: String,
    },
}

impl fmt::Display for FormatterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::SyntaxError(message) => write!(f, "syntax error: {message}"),
            Self::ConfigRead { path, source } => {
                write!(
                    f,
                    "failed to read config {}: {source}",
                    path.to_string_lossy()
                )
            }
            Self::ConfigParse { path, message } => {
                if let Some(path) = path {
                    write!(
                        f,
                        "failed to parse config {}: {message}",
                        path.to_string_lossy()
                    )
                } else {
                    write!(f, "failed to parse config: {message}")
                }
            }
            Self::GlobPattern { pattern, message } => {
                write!(f, "invalid glob pattern {pattern:?}: {message}")
            }
        }
    }
}

impl std::error::Error for FormatterError {}

impl From<io::Error> for FormatterError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn format_text(code: &str, level: LuaLanguageLevel, config: &LuaFormatConfig) -> FormatOutput {
    let check = check_text(code, level, config);
    FormatOutput {
        formatted: check.formatted,
        changed: check.changed,
    }
}

pub fn check_text(
    code: &str,
    level: LuaLanguageLevel,
    config: &LuaFormatConfig,
) -> FormatCheckResult {
    let source = SourceText { text: code, level };
    let formatted = reformat_lua_code(&source, config);
    let changed = formatted != code;
    let changed_line_ranges = if changed {
        collect_changed_line_ranges(code, &formatted)
    } else {
        Vec::new()
    };
    FormatCheckResult {
        formatted,
        changed,
        changed_line_ranges,
    }
}

pub fn format_text_for_path(
    code: &str,
    level: LuaLanguageLevel,
    source_path: Option<&Path>,
    explicit_config_path: Option<&Path>,
) -> Result<FormatPathResult, FormatterError> {
    let result = check_text_for_path(code, level, source_path, explicit_config_path)?;
    Ok(FormatPathResult {
        path: result.path,
        output: FormatOutput {
            formatted: result.output.formatted,
            changed: result.output.changed,
        },
        config_path: result.config_path,
    })
}

pub fn check_text_for_path(
    code: &str,
    level: LuaLanguageLevel,
    source_path: Option<&Path>,
    explicit_config_path: Option<&Path>,
) -> Result<FormatCheckPathResult, FormatterError> {
    let resolved = resolve_config_for_path(source_path, explicit_config_path)?;
    let output = check_text(code, level, &resolved.config);
    Ok(FormatCheckPathResult {
        path: source_path
            .unwrap_or_else(|| Path::new("<memory>"))
            .to_path_buf(),
        output,
        config_path: resolved.source_path,
    })
}

pub fn format_file(
    path: &Path,
    level: LuaLanguageLevel,
    explicit_config_path: Option<&Path>,
) -> Result<FormatPathResult, FormatterError> {
    let result = check_file(path, level, explicit_config_path)?;
    Ok(FormatPathResult {
        path: result.path,
        output: FormatOutput {
            formatted: result.output.formatted,
            changed: result.output.changed,
        },
        config_path: result.config_path,
    })
}

pub fn check_file(
    path: &Path,
    level: LuaLanguageLevel,
    explicit_config_path: Option<&Path>,
) -> Result<FormatCheckPathResult, FormatterError> {
    let source = fs::read_to_string(path)?;
    let resolved = resolve_config_for_path(Some(path), explicit_config_path)?;
    let output = check_text(&source, level, &resolved.config);
    Ok(FormatCheckPathResult {
        path: path.to_path_buf(),
        output,
        config_path: resolved.source_path,
    })
}

pub fn default_config_toml() -> Result<String, FormatterError> {
    to_toml_string(&LuaFormatConfig::default()).map_err(|err| FormatterError::ConfigParse {
        path: None,
        message: format!("failed to serialize default config: {err}"),
    })
}

pub fn parse_format_config(
    content: &str,
    path: Option<&Path>,
) -> Result<LuaFormatConfig, FormatterError> {
    let ext = path
        .and_then(|value| value.extension())
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "toml" => {
            from_toml_str::<LuaFormatConfig>(content).map_err(|err| FormatterError::ConfigParse {
                path: path.map(Path::to_path_buf),
                message: err.to_string(),
            })
        }
        "json" => serde_json::from_str::<LuaFormatConfig>(content).map_err(|err| {
            FormatterError::ConfigParse {
                path: path.map(Path::to_path_buf),
                message: err.to_string(),
            }
        }),
        "yml" | "yaml" => serde_yml::from_str::<LuaFormatConfig>(content).map_err(|err| {
            FormatterError::ConfigParse {
                path: path.map(Path::to_path_buf),
                message: err.to_string(),
            }
        }),
        _ => try_parse_unknown_config_format(content, path),
    }
}

pub fn load_format_config(path: &Path) -> Result<LuaFormatConfig, FormatterError> {
    let content = fs::read_to_string(path).map_err(|source| FormatterError::ConfigRead {
        path: path.to_path_buf(),
        source,
    })?;
    parse_format_config(&content, Some(path))
}

pub fn discover_config_path(start: &Path) -> Option<PathBuf> {
    let root = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(start)
    };

    for dir in root.ancestors() {
        for file_name in CONFIG_FILE_NAMES {
            let path = dir.join(file_name);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

pub fn resolve_config_for_path(
    source_path: Option<&Path>,
    explicit_config_path: Option<&Path>,
) -> Result<ResolvedConfig, FormatterError> {
    if let Some(path) = explicit_config_path {
        return Ok(ResolvedConfig {
            config: load_format_config(path)?,
            source_path: Some(path.to_path_buf()),
        });
    }

    if let Some(source_path) = source_path
        && let Some(path) = discover_config_path(source_path)
    {
        return Ok(ResolvedConfig {
            config: load_format_config(&path)?,
            source_path: Some(path),
        });
    }

    Ok(ResolvedConfig {
        config: LuaFormatConfig::default(),
        source_path: None,
    })
}

pub fn collect_lua_files(
    inputs: &[PathBuf],
    options: &FileCollectorOptions,
) -> Result<Vec<PathBuf>, FormatterError> {
    let include_patterns = compile_patterns(&options.include)?;
    let mut exclude_values = options.exclude.clone();
    if options.respect_ignore_files {
        exclude_values.extend(load_ignore_patterns(inputs)?);
    }
    let exclude_patterns = compile_patterns(&exclude_values)?;

    let mut files = BTreeSet::new();
    for input in inputs {
        if input.is_file() {
            let root = input.parent().unwrap_or(input.as_path());
            if should_include_file(input, root, options, &include_patterns, &exclude_patterns) {
                files.insert(input.clone());
            }
            continue;
        }

        if !input.exists() {
            return Err(FormatterError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("path not found: {}", input.to_string_lossy()),
            )));
        }

        if !input.is_dir() {
            continue;
        }

        let walker = WalkDir::new(input)
            .follow_links(options.follow_symlinks)
            .max_depth(if options.recursive { usize::MAX } else { 1 })
            .into_iter()
            .filter_entry(|entry| should_walk_entry(entry, options));

        for entry in walker {
            let entry = entry.map_err(|err| FormatterError::Io(io::Error::other(err)))?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            if should_include_file(path, input, options, &include_patterns, &exclude_patterns) {
                files.insert(path.to_path_buf());
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn try_parse_unknown_config_format(
    content: &str,
    path: Option<&Path>,
) -> Result<LuaFormatConfig, FormatterError> {
    from_toml_str::<LuaFormatConfig>(content)
        .or_else(|_| serde_json::from_str::<LuaFormatConfig>(content))
        .or_else(|_| serde_yml::from_str::<LuaFormatConfig>(content))
        .map_err(|err| FormatterError::ConfigParse {
            path: path.map(Path::to_path_buf),
            message: format!("unknown extension, failed to parse as TOML/JSON/YAML: {err}"),
        })
}

fn compile_patterns(patterns: &[String]) -> Result<Vec<Pattern>, FormatterError> {
    patterns
        .iter()
        .map(|pattern| {
            Pattern::new(pattern).map_err(|err| FormatterError::GlobPattern {
                pattern: pattern.clone(),
                message: err.to_string(),
            })
        })
        .collect()
}

fn should_walk_entry(entry: &DirEntry, options: &FileCollectorOptions) -> bool {
    if entry.depth() == 0 {
        return true;
    }

    let file_name = entry.file_name().to_string_lossy();
    if entry.file_type().is_dir() {
        if DEFAULT_IGNORED_DIRS.contains(&file_name.as_ref()) {
            return false;
        }
        if !options.include_hidden && file_name.starts_with('.') {
            return false;
        }
    } else if !options.include_hidden && file_name.starts_with('.') {
        return false;
    }

    true
}

fn should_include_file(
    path: &Path,
    root: &Path,
    options: &FileCollectorOptions,
    include_patterns: &[Pattern],
    exclude_patterns: &[Pattern],
) -> bool {
    if !options.include_hidden && has_hidden_component(path, root) {
        return false;
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("lua") | Some("luau")) {
        return false;
    }

    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_display = normalize_path(relative);
    let absolute_display = normalize_path(path);

    if is_match(exclude_patterns, &relative_display)
        || is_match(exclude_patterns, &absolute_display)
    {
        return false;
    }

    if include_patterns.is_empty() {
        return true;
    }

    is_match(include_patterns, &relative_display) || is_match(include_patterns, &absolute_display)
}

fn is_match(patterns: &[Pattern], candidate: &str) -> bool {
    patterns.iter().any(|pattern| pattern.matches(candidate))
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn has_hidden_component(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
}

fn load_ignore_patterns(inputs: &[PathBuf]) -> Result<Vec<String>, FormatterError> {
    let mut paths = BTreeSet::new();
    for input in inputs {
        let start = if input.is_dir() {
            input.as_path()
        } else {
            input.parent().unwrap_or(input.as_path())
        };

        if let Some(path) = discover_ignore_path(start) {
            paths.insert(path);
        }
    }

    let mut patterns = Vec::new();
    for path in paths {
        let content = fs::read_to_string(&path).map_err(|source| FormatterError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        patterns.extend(parse_ignore_file(&content));
    }
    Ok(patterns)
}

fn discover_ignore_path(start: &Path) -> Option<PathBuf> {
    let root = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(start)
    };

    for dir in root.ancestors() {
        let path = dir.join(IGNORE_FILE_NAME);
        if path.is_file() {
            return Some(path);
        }
    }

    None
}

fn parse_ignore_file(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

fn collect_changed_line_ranges(original: &str, formatted: &str) -> Vec<ChangedLineRange> {
    let original_lines: Vec<&str> = original.lines().collect();
    let formatted_lines: Vec<&str> = formatted.lines().collect();
    let max_len = original_lines.len().max(formatted_lines.len());

    let mut ranges = Vec::new();
    let mut current_start: Option<usize> = None;

    for index in 0..max_len {
        let original_line = original_lines.get(index).copied();
        let formatted_line = formatted_lines.get(index).copied();
        if original_line != formatted_line {
            if current_start.is_none() {
                current_start = Some(index + 1);
            }
        } else if let Some(start_line) = current_start.take() {
            ranges.push(ChangedLineRange {
                start_line,
                end_line: index,
            });
        }
    }

    if let Some(start_line) = current_start {
        ranges.push(ChangedLineRange {
            start_line,
            end_line: max_len.max(start_line),
        });
    }

    ranges
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{unique}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn test_collect_lua_files_recurses_and_ignores_defaults() {
        let root = make_temp_dir("luafmt-files");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("a.lua"), "local a=1\n").unwrap();
        fs::write(root.join("nested").join("b.luau"), "local b=2\n").unwrap();
        fs::write(root.join("nested").join("c.txt"), "noop\n").unwrap();
        fs::write(root.join("target").join("skip.lua"), "local c=3\n").unwrap();

        let files = collect_lua_files(
            std::slice::from_ref(&root),
            &FileCollectorOptions::default(),
        )
        .unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|path| path.ends_with("a.lua")));
        assert!(files.iter().any(|path| path.ends_with("b.luau")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_collect_lua_files_respects_ignore_file_and_globs() {
        let root = make_temp_dir("luafmt-ignore");
        fs::create_dir_all(root.join("gen")).unwrap();
        fs::write(root.join(".luafmtignore"), "gen/**\nignore.lua\n").unwrap();
        fs::write(root.join("keep.lua"), "local keep=1\n").unwrap();
        fs::write(root.join("ignore.lua"), "local ignore=1\n").unwrap();
        fs::write(
            root.join("gen").join("generated.lua"),
            "local generated=1\n",
        )
        .unwrap();

        let options = FileCollectorOptions {
            include: vec!["**/*.lua".to_string()],
            ..Default::default()
        };
        let files = collect_lua_files(std::slice::from_ref(&root), &options).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("keep.lua"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_resolve_config_for_path_discovers_nearest_config() {
        let root = make_temp_dir("luafmt-config");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".luafmt.toml"), "[layout]\nmax_line_width = 88\n").unwrap();
        let file_path = root.join("src").join("main.lua");
        fs::write(&file_path, "local x=1\n").unwrap();

        let resolved = resolve_config_for_path(Some(&file_path), None).unwrap();

        assert_eq!(resolved.config.layout.max_line_width, 88);
        assert_eq!(resolved.source_path, Some(root.join(".luafmt.toml")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_check_text_reports_formatted_output_and_changed_flag() {
        let config = LuaFormatConfig::default();

        let result = check_text("local x=1\n", LuaLanguageLevel::default(), &config);

        assert!(result.changed);
        assert_eq!(result.formatted, "local x = 1\n");
        assert_eq!(result.changed_line_ranges.len(), 1);
        assert_eq!(result.changed_line_ranges[0].start_line, 1);
        assert_eq!(result.changed_line_ranges[0].end_line, 1);
    }

    #[test]
    fn test_check_text_for_path_uses_discovered_config() {
        let root = make_temp_dir("luafmt-check-config");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".luafmt.toml"), "[layout]\nmax_line_width = 10\n").unwrap();
        let file_path = root.join("src").join("main.lua");
        fs::write(&file_path, "call(alpha, beta, gamma)\n").unwrap();

        let result = check_file(&file_path, LuaLanguageLevel::default(), None).unwrap();

        assert!(result.output.changed);
        assert_eq!(result.config_path, Some(root.join(".luafmt.toml")));
        assert!(result.output.formatted.contains("\n"));
        assert!(!result.output.changed_line_ranges.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn test_check_text_collects_multiple_changed_line_ranges() {
        let ranges = collect_changed_line_ranges(
            "local a=1\nlocal b=2\nprint(a+b)\n",
            "local a = 1\nlocal b = 2\nprint(a + b)\n",
        );

        assert_eq!(ranges.len(), 1);
        assert_eq!(
            ranges[0],
            ChangedLineRange {
                start_line: 1,
                end_line: 3
            }
        );
    }
}
