use emmylua_doc_cli::{CmdArgs, Parser, run_doc_cli};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd_args = CmdArgs::parse();
    run_doc_cli(cmd_args)
}
