use schema_to_emmylua::SchemaConverter;
use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: schema_to_emmylua <schema.json> [output.lua]");
        eprintln!();
        eprintln!("Converts a JSON Schema file into LuaLS (EmmyLua) annotations.");
        eprintln!();
        eprintln!("Arguments:");
        eprintln!("  <schema.json>  Path to the input JSON Schema file");
        eprintln!("  [output.lua]   Optional path for output file (defaults to stdout)");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let json_str = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Failed to read '{}': {}", input_path, e);
        std::process::exit(1);
    });

    let converter = SchemaConverter::new(false);
    let output = converter.convert_from_str(&json_str).unwrap_or_else(|e| {
        eprintln!("Failed to parse JSON Schema: {}", e);
        std::process::exit(1);
    });

    if let Some(output_path) = args.get(2) {
        fs::write(output_path, &output).unwrap_or_else(|e| {
            eprintln!("Failed to write '{}': {}", output_path, e);
            std::process::exit(1);
        });
        eprintln!("Written to {}", output_path);
    } else {
        print!("{}", output);
    }
}
