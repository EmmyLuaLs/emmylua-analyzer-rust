use std::fs;
use toml_edit::{value, DocumentMut};

const CARGOS: &[&str] = &[
    "crates/emmylua_code_analysis/Cargo.toml",
    "crates/emmylua_doc_cli/Cargo.toml",
    "crates/emmylua_ls/Cargo.toml",
    "crates/emmylua_check/Cargo.toml",
    "crates/emmylua_formatter/Cargo.toml",
];

fn main() {
    let mut version = std::env::args().nth(1).expect("Please provide a version");
    if version.starts_with("refs/tags/") {
        version = version.replace("refs/tags/", "");
    }

    let current_dir = std::env::current_dir().unwrap();
    // 向上查找到有crates的目录
    let workspace_dir = current_dir
        .ancestors()
        .find(|dir| dir.join("crates").exists())
        .expect("Unable to find crates directory");

    for cargo in CARGOS {
        let path = workspace_dir.join(cargo);
        let content =
            fs::read_to_string(&path).unwrap_or_else(|_| panic!("Unable to read {}", cargo));

        let mut doc = content
            .parse::<DocumentMut>()
            .unwrap_or_else(|_| panic!("Failed to parse {}", cargo));

        doc["package"]["version"] = value(version.clone());

        fs::write(&path, doc.to_string())
            .unwrap_or_else(|_| panic!("Unable to write to {}", cargo));
    }

    let workspacec_cargo = workspace_dir.join("Cargo.toml");
    let content = fs::read_to_string(&workspacec_cargo)
        .unwrap_or_else(|_| panic!("Unable to read {:?}", workspacec_cargo));
    let mut doc = content
        .parse::<DocumentMut>()
        .unwrap_or_else(|_| panic!("Failed to parse {:?}", workspacec_cargo));

    let dependencies = doc["workspace"]["dependencies"].as_table_mut().unwrap();
    if let Some(dep) = dependencies.get_mut("emmylua_code_analysis") {
        dep["version"] = value(version.clone());
    }

    fs::write(&workspacec_cargo, doc.to_string())
        .unwrap_or_else(|_| panic!("Unable to write to {:?}", workspacec_cargo));
}
