use std::path::PathBuf;

mod comment_syntax;
mod extractor;
mod keys;
mod merger;
mod meta;
mod model;
mod translator;

fn main() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("tools/std_i18n is two levels under repo root")
        .to_path_buf();

    let std_dir = repo_root.join("crates/emmylua_code_analysis/resources/std");
    let out_root = repo_root.join("crates/emmylua_ls/std_i18n");

    // zh_CN
    merger::write_std_locales_yaml(&std_dir, "zh_CN", &out_root, true)
        .expect("write std zh_CN locales should succeed");
    // meta
    meta::write_std_meta_yaml(&std_dir, &out_root).expect("write std meta.yaml should succeed");
}
