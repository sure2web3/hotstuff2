use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use log::{debug, error, info};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Sleep};

use crate::error::HotStuffError;
use crate::message::consensus::Timeout;
use crate::types::QuorumCert;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeoutKey {
    pub height: u64,
    pub round: u64,
}

pub struct TimeoutManager {
    timeout_sender: mpsc::Sender<TimeoutCommand>,
    timeout_receiver: tokio::sync::Mutex<Option<mpsc::Receiver<TimeoutCommand>>>,
    timeouts: Arc<DashMap<TimeoutKey, oneshot::Sender<()>>>,
    base_timeout: Duration,
    timeout_multiplier: f64,
}

#[derive(Debug)]
enum TimeoutCommand {
    StartTimeout {
        key: TimeoutKey,
        response_sender: oneshot::Sender<Result<(), HotStuffError>>,
    },
    CancelTimeout {
        key: TimeoutKey,
        response_sender: oneshot::Sender<Result<(), HotStuffError>>,
    },
    Shutdown,
}

impl TimeoutManager {
    pub fn new(base_timeout: Duration, timeout_multiplier: f64) -> Arc<Self> {
        let (timeout_sender, timeout_receiver) = mpsc::channel(100);
        Arc::new(Self {
            timeout_sender,
            timeout_receiver: tokio::sync::Mutex::new(Some(timeout_receiver)),
            timeouts: Arc::new(DashMap::new()),
            base_timeout,
            timeout_multiplier,
        })
    }

    pub fn start(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut receiver = this
                .timeout_receiver
                .lock()
                .await
                .take()
                .expect("TimeoutManager already started");
            while let Some(command) = receiver.recv().await {
                match command {
                    TimeoutCommand::StartTimeout {
                        key,
                        response_sender,
                    } => {
                        let result = this.handle_start_timeout(key).await;
                        let _ = response_sender.send(result);
                    }
                    TimeoutCommand::CancelTimeout {
                        key,
                        response_sender,
                    } => {
                        let result = this.handle_cancel_timeout(key);
                        let _ = response_sender.send(result);
                    }
                    TimeoutCommand::Shutdown => {
                        info!("TimeoutManager shutting down");
                        break;
                    }
                }
            }
        });
    }

    async fn handle_start_timeout(&self, key: TimeoutKey) -> Result<(), HotStuffError> {
        let timeout_duration = self
            .base_timeout
            .mul_f64(self.timeout_multiplier.powi(key.round as i32));
        if self.timeouts.contains_key(&key) {
            return Err(HotStuffError::Timer(
                "Timeout already exists for this key".to_string(),
            ));
        }
        let (cancel_sender, mut cancel_receiver) = oneshot::channel();
        self.timeouts.insert(key, cancel_sender);

        let timeouts = Arc::clone(&self.timeouts);
        tokio::spawn(async move {
            let timeout_future = sleep(timeout_duration);
            tokio::pin!(timeout_future);

            tokio::select! {
                _ = &mut timeout_future => {
                    timeouts.remove(&key);
                    info!("Timeout expired for height {}, round {}", key.height, key.round);
                    // TODO: Send timeout message
                }
                _ = &mut cancel_receiver => {
                    info!("Timeout cancelled for height {}, round {}", key.height, key.round);
                }
            }
        });

        Ok(())
    }

    fn handle_cancel_timeout(&self, key: TimeoutKey) -> Result<(), HotStuffError> {
        if let Some((_, sender)) = self.timeouts.remove(&key) {
            let _ = sender.send(());
            Ok(())
        } else {
            Err(HotStuffError::Timer(
                "No timeout found for this key".to_string(),
            ))
        }
    }

    pub async fn start_timeout(&self, height: u64, round: u64) -> Result<(), HotStuffError> {
        let key = TimeoutKey { height, round };
        let (response_sender, response_receiver) = oneshot::channel();

        self.timeout_sender
            .send(TimeoutCommand::StartTimeout {
                key,
                response_sender,
            })
            .await
            .map_err(|e| {
                HotStuffError::Timer(format!("Failed to send start timeout command: {}", e))
            })?;

        response_receiver.await.map_err(|e| {
            HotStuffError::Timer(format!("Failed to receive start timeout response: {}", e))
        })?
    }

    pub async fn cancel_timeout(&self, height: u64, round: u64) -> Result<(), HotStuffError> {
        let key = TimeoutKey { height, round };
        let (response_sender, response_receiver) = oneshot::channel();

        self.timeout_sender
            .send(TimeoutCommand::CancelTimeout {
                key,
                response_sender,
            })
            .await
            .map_err(|e| {
                HotStuffError::Timer(format!("Failed to send cancel timeout command: {}", e))
            })?;

        response_receiver.await.map_err(|e| {
            HotStuffError::Timer(format!("Failed to receive cancel timeout response: {}", e))
        })?
    }

    pub async fn shutdown(&self) {
        let _ = self.timeout_sender.send(TimeoutCommand::Shutdown).await;
    }
}
