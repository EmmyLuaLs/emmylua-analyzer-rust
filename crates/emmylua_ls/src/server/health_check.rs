use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::context::ClientProxy;
use lsp_types::{MessageType, ShowMessageParams};

pub struct HealthCheck {
    last_heartbeat: Arc<AtomicU64>,
    last_warning: Arc<AtomicU64>,
}

impl HealthCheck {
    pub fn new() -> Self {
        Self {
            last_heartbeat: Arc::new(AtomicU64::new(0)),
            last_warning: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn heartbeat(&self) {
        self.last_heartbeat.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::Release,
        );
    }

    pub fn start_monitoring(self: Arc<Self>, client: Arc<ClientProxy>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let last = self.last_heartbeat.load(Ordering::Acquire);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                if last > 0 && now - last > 120 {
                    let last_warn = self.last_warning.load(Ordering::Acquire);
                    if now - last_warn > 300 {
                        self.last_warning.store(now, Ordering::Release);
                        log::warn!("Health check: {}s since last heartbeat", now - last);
                        client.show_message(ShowMessageParams {
                            typ: MessageType::WARNING,
                            message: format!(
                                "Language server busy for {}s. Performance may be slow.",
                                now - last
                            ),
                        });
                    }
                }
            }
        });
    }
}
