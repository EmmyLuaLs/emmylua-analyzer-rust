use std::{collections::HashMap, path::Path};

use include_dir::{Dir, include_dir};
use tera::Tera;

static HTML_TEMPLATE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/template/html");
static STATIC_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/template/static");

pub fn init_html_tl() -> Option<Tera> {
    let mut tera = Tera::default();
    tera.autoescape_on(vec![]);
    let files: HashMap<String, String> = HTML_TEMPLATE_DIR
        .files()
        .map(|file| {
            let path = file.path().to_string_lossy().into_owned();
            let content = file.contents_utf8().unwrap().to_string();
            (path, content)
        })
        .collect();
    if files.is_empty() {
        log::error!("No HTML templates found in embedded directory");
        return None;
    }
    match tera.add_raw_templates(files) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to add HTML templates: {}", e);
            return None;
        }
    }
    Some(tera)
}

pub fn write_static_assets(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let static_dir = output.join("static");
    std::fs::create_dir_all(&static_dir)?;
    for file in STATIC_DIR.files() {
        let name = file.path().file_name().unwrap().to_str().unwrap();
        std::fs::write(static_dir.join(name), file.contents())?;
    }
    Ok(())
}
