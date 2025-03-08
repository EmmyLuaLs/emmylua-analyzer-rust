use tera::Tera;

use super::MkdocsIndex;

pub fn generate_index(
    tl: &Tera,
    mkdocs: &mut MkdocsIndex,
    output: &std::path::PathBuf,
) -> Option<()> {
    let mut context = tera::Context::new();
    mkdocs.types.sort_by(|a, b| a.name.cmp(&b.name));
    mkdocs.modules.sort_by(|a, b| a.name.cmp(&b.name));
    mkdocs.globals.sort_by(|a, b| a.name.cmp(&b.name));

    if !mkdocs.types.is_empty() {
        context.insert("types", &mkdocs.types);
    }
    if !mkdocs.modules.is_empty() {
        context.insert("modules", &mkdocs.modules);
    }
    if !mkdocs.globals.is_empty() {
        context.insert("globals", &mkdocs.globals);
    }
    let index_path = output.join("docs/index.md");
    let index_text = match tl.render("index_template.tl", &context) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("Failed to render index: {}", e);
            return None;
        }
    };

    std::fs::write(index_path, index_text).ok()?;

    let mkdocs_yml_path = output.join("mkdocs.yml");
    let mkdocs_yml_text = match tl.render("mkdocs_template.tl", &context) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("Failed to render mkdocs.yml: {}", e);
            return None;
        }
    };

    std::fs::write(mkdocs_yml_path, mkdocs_yml_text).ok()?;

    Some(())
}
