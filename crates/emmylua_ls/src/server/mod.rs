mod connection;
mod error;
mod lsp_server;
mod main_loop;
mod message_processor;

pub use connection::AsyncConnection;
pub use error::ExitError;

use lsp_types::InitializeParams;
use std::error::Error;

use crate::cmd_args::{self, CmdArgs};
use crate::handlers::server_capabilities;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[allow(unused)]
pub async fn run_ls(cmd_args: CmdArgs) -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, threads) = match cmd_args.communication {
        cmd_args::Communication::Stdio => ::lsp_server::Connection::stdio(),
        cmd_args::Communication::Tcp => {
            let port = cmd_args.port;
            let ip = cmd_args.ip.clone();
            let addr = (ip.as_str(), port);
            ::lsp_server::Connection::listen(addr).unwrap()
        }
    };

    let (id, params) = connection.initialize_start()?;
    let initialization_params: InitializeParams = serde_json::from_value(params).unwrap();
    let server_capabilities = server_capabilities(&initialization_params.capabilities);
    let initialize_data = serde_json::json!({
        "capabilities": server_capabilities,
        "serverInfo": {
            "name": CRATE_NAME,
            "version": CRATE_VERSION
        }
    });

    connection.initialize_finish(id, initialize_data)?;

    // Create async connection wrapper
    let async_connection = AsyncConnection::from_sync(connection);
    main_loop::main_loop(async_connection, initialization_params, cmd_args).await?;
    threads.join()?;

    eprintln!("Server shutting down.");
    Ok(())
}
