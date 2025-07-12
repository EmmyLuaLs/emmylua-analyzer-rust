use crate::OutputDestination;
use emmylua_code_analysis::EmmyLuaAnalysis;

mod export;
mod json_types;

pub fn generate_json(
    analysis: &EmmyLuaAnalysis,
    output: OutputDestination,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = analysis.compilation.get_db();

    let output = match output {
        OutputDestination::File(output) if output.extension() == Some("json".as_ref()) => {
            if let Some(parent) = output.parent() {
                if !parent.exists() {
                    log::info!("Creating output directory: {:?}", parent);
                    std::fs::create_dir_all(&parent)?;
                }
            }

            OutputDestination::File(output)
        }
        OutputDestination::File(output) => {
            if !output.exists() {
                log::info!("Creating output directory: {:?}", output);
                std::fs::create_dir_all(&output)?;
            }

            OutputDestination::File(output.join("doc.json"))
        }
        OutputDestination::Stdout => OutputDestination::Stdout,
    };

    let data = export::export(db);

    match output {
        OutputDestination::Stdout => {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        OutputDestination::File(json_path) => {
            log::info!("Writing JSON to: {:?}", json_path);
            std::fs::write(&json_path, serde_json::to_string_pretty(&data)?)?;
            eprintln!("Documentation JSON exported to {:?}", json_path);
        }
    }

    Ok(())
}
