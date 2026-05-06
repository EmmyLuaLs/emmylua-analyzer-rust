use std::path::PathBuf;

mod analysis;
mod diagnostics;
mod doc_text;
mod fs;
mod keys;
mod locale_yaml;
mod meta_format;
mod model;
mod pipeline;
mod target;

fn main() {
    // 是否全量输出翻译条目：
    // - `true`：输出所有提取到的 key（包含没有原文的条目）
    // - `false`：不输出不包含原文的 key（默认，压缩输出体积）
    let full_output = false;

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("tools/std_i18n is two levels under repo root")
        .to_path_buf();

    let std_dir = repo_root.join("crates/emmylua_code_analysis/resources/std");
    let out_root = repo_root.join("crates/emmylua_ls/std_i18n");

    let mut diagnostics = diagnostics::Diagnostics::default();
    let project = pipeline::analyze_std_i18n_project(&std_dir, full_output, &mut diagnostics)
        .expect("analyze std i18n project should succeed");

    // zh_CN
    locale_yaml::write_std_locales_yaml(&project, "zh_CN", &out_root)
        .expect("write std zh_CN locales should succeed");
    // meta
    meta_format::write_std_meta_yaml(&project, &out_root)
        .expect("write std meta.yaml should succeed");

    diagnostics.emit();
}
