use std::{collections::HashMap, path::PathBuf};

use include_dir::{include_dir, Dir};
use tera::Tera;

static TEMPLATE_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/template");

pub fn init_tl(override_template: Option<PathBuf>) -> Option<Tera> {
    let mut tera = Tera::default();
    let mut files: HashMap<String, String> = TEMPLATE_DIR
        .files()
        .map(|file| {
            let path = file.path().to_string_lossy().into_owned();
            let content = file.contents_utf8().unwrap().to_string();
            (path, content)
        })
        .collect();

    if let Some(override_template) = override_template {
        if !override_template.exists() {
            log::error!(
                "Override template directory does not exist: {:?}",
                override_template
            );
            return None;
        }

        if override_template.is_dir() {
            for entry in walkdir::WalkDir::new(&override_template)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path();
                let content = std::fs::read_to_string(path).expect("Failed to read file");
                let template_path = path.file_name().unwrap().to_str().unwrap().to_string();
                files.insert(template_path, content);
            }
        }
    }

    match tera.add_raw_templates(files) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Failed to add templates: {}", e);
            return None;
        }
    }

    Some(tera)
}
