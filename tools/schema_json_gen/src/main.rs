use emmylua_code_analysis::Emmyrc;
use std::fs;

fn main() {
    let schema = schemars::schema_for!(Emmyrc);
    let schema_json = serde_json::to_string_pretty(&schema).unwrap();
    let root_crates = std::env::current_dir().unwrap();
    let output_path = root_crates.join("crates/emmylua_code_analysis/resources/schema.json");
    println!("Output path: {:?}", output_path);
    fs::write(output_path, schema_json).expect("Unable to write file");
}
