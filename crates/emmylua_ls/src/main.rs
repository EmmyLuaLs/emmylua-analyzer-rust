mod cmd_args;
mod context;
mod handlers;
mod logger;
mod meta_text;
mod util;

use cmd_args::CmdArgs;
use handlers::{
    initialized_handler, on_notification_handler, on_req_handler, on_response_handler,
    server_capabilities,
};
use lsp_server::{Connection, Message};
use lsp_types::InitializeParams;
use std::{env, error::Error};
use structopt::StructOpt;

#[macro_use]
extern crate rust_i18n;
rust_i18n::i18n!("./locales", fallback = "en");

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let cmd_args = CmdArgs::from_args();
    let (connection, threads) = match cmd_args.communication {
        cmd_args::Communication::Stdio => Connection::stdio(),
        cmd_args::Communication::Tcp => {
            let port = cmd_args.port;
            let ip = cmd_args.ip.clone();
            let addr = (ip.as_str(), port);
            Connection::listen(addr).unwrap()
        }
    };

    let (id, params) = connection.initialize_start()?;
    let initialization_params: InitializeParams = serde_json::from_value(params).unwrap();
    let server_capbilities = server_capabilities(&initialization_params.capabilities);
    let initialize_data = serde_json::json!({
        "capabilities": server_capbilities,
        "serverInfo": {
            "name": CRATE_NAME,
            "version": CRATE_VERSION
        }
    });

    connection.initialize_finish(id, initialize_data)?;

    main_loop(connection, initialization_params, cmd_args).await?;
    threads.join()?;

    eprintln!("Server shutting down.");
    Ok(())
}

async fn main_loop(
    connection: Connection,
    params: InitializeParams,
    cmd_args: CmdArgs,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut server_context = context::ServerContext::new(Connection {
        sender: connection.sender.clone(),
        receiver: connection.receiver.clone(),
    });

    let server_context_snapshot = server_context.snapshot();
    tokio::spawn(async move {
        initialized_handler(server_context_snapshot, params, cmd_args).await;
    });

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    server_context.close().await;
                    return Ok(());
                }

                on_req_handler(req, &mut server_context).await?;
            }
            Message::Notification(notify) => {
                on_notification_handler(notify, &mut server_context).await?;
            }
            Message::Response(response) => {
                on_response_handler(response, &mut server_context).await?;
            }
        }
    }

    server_context.close().await;
    Ok(())
}
