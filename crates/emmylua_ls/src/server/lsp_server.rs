use lsp_types::InitializeParams;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::context;

use super::connection::AsyncConnection;
use super::health_check::HealthCheck;
use super::message_processor::ServerMessageProcessor;

pub(super) struct LspServer {
    pub(super) connection: AsyncConnection,
    pub(super) server_context: context::ServerContext,
    pub(super) processor: ServerMessageProcessor,
}

impl LspServer {
    pub(super) fn new(
        connection: AsyncConnection,
        params: &InitializeParams,
        init_rx: oneshot::Receiver<()>,
    ) -> Self {
        let server_context = context::ServerContext::new(
            lsp_server::Connection {
                sender: connection.connection.sender.clone(),
                receiver: connection.connection.receiver.clone(),
            },
            params.capabilities.clone(),
        );

        Self {
            connection,
            server_context,
            processor: ServerMessageProcessor::new(init_rx),
        }
    }

    pub(super) async fn run(mut self) -> Result<(), Box<dyn Error + Sync + Send>> {
        let health_check = Arc::new(HealthCheck::new());
        let snapshot = self.server_context.snapshot();
        let client_arc = snapshot.client_arc();
        health_check.clone().start_monitoring(client_arc);

        self.wait_for_initialization().await?;

        if self
            .processor
            .process_pending_messages(&mut self.connection, &mut self.server_context)
            .await?
        {
            self.server_context.close().await;
            return Ok(());
        }

        while let Some(msg) = self.connection.recv().await {
            health_check.heartbeat();
            if self
                .processor
                .process_message(msg, &mut self.connection, &mut self.server_context)
                .await?
            {
                break;
            }
        }

        self.server_context.close().await;
        Ok(())
    }

    async fn wait_for_initialization(&mut self) -> Result<(), Box<dyn Error + Sync + Send>> {
        loop {
            if self.processor.check_initialization_complete()? {
                break;
            }

            match tokio::time::timeout(
                tokio::time::Duration::from_millis(50),
                self.connection.recv(),
            )
            .await
            {
                Ok(Some(msg)) => {
                    if self.processor.can_process_during_init(&msg) {
                        self.processor
                            .handle_message(msg, &mut self.connection, &mut self.server_context)
                            .await?;
                    } else {
                        self.processor.pending_messages.push(msg);
                    }
                }
                Ok(None) => {
                    return Ok(());
                }
                Err(_) => {
                    continue;
                }
            }
        }
        Ok(())
    }
}
