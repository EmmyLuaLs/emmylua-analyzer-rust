use emmylua_code_analysis::EmmyLuaAnalysis;
use std::path::PathBuf;

mod export;
mod json_types;

pub fn generate_json(
    analysis: &mut EmmyLuaAnalysis,
    output: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = analysis.compilation.get_db();

    let json_path = if output.extension() == Some("json".as_ref()) {
        if let Some(parent) = output.parent() {
            if !parent.exists() {
                eprintln!("Creating output directory: {:?}", parent);
                std::fs::create_dir_all(&parent)?;
            }
        }

        output
    } else {
        if !output.exists() {
            eprintln!("Creating output directory: {:?}", output);
            std::fs::create_dir_all(&output)?;
        }

        output.join("doc.json")
    };

    let data = export::export(db);

    eprintln!("Writing JSON output to {:?}", json_path);

    std::fs::write(&json_path, serde_json::to_string_pretty(&data)?)?;

    Ok(())
}
