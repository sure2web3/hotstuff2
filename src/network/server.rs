use futures::StreamExt;
use log::{error, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::error::HotStuffError;
use crate::message::network::NetworkMsg;
use crate::message::network::PeerDiscoveryMsg;

pub struct NetworkServer {
    listener: TcpListener,
    message_sender: mpsc::Sender<(u64, NetworkMsg)>,
    node_id: u64,
}

impl NetworkServer {
    pub async fn new(
        addr: &str,
        node_id: u64,
        message_sender: mpsc::Sender<(u64, NetworkMsg)>,
    ) -> Result<Self, HotStuffError> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| HotStuffError::Network(format!("Failed to bind to {}: {}", addr, e)))?;

        info!("Network server listening on {}", addr);

        Ok(Self {
            listener,
            message_sender,
            node_id,
        })
    }

    pub async fn start(&self) -> Result<(), HotStuffError> {
        info!("Starting network server for node {}", self.node_id);

        loop {
            let (stream, peer_addr) = self.listener.accept().await.map_err(|e| {
                HotStuffError::Network(format!("Failed to accept connection: {}", e))
            })?;

            info!("New connection from {}", peer_addr);

            let message_sender = self.message_sender.clone();

            // Spawn a new task to handle the connection
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, message_sender).await {
                    error!("Error handling connection: {}", e);
                }
            });
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        message_sender: mpsc::Sender<(u64, NetworkMsg)>,
    ) -> Result<(), HotStuffError> {
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        while let Some(data) = framed.next().await {
            let data = data.map_err(|e| HotStuffError::Network(format!("Read error: {}", e)))?;

            let msg: NetworkMsg = bincode::deserialize(&data)?;

            // Extract sender ID (this is a simplification, in reality we'd need proper authentication)
            let sender_id = match &msg {
                NetworkMsg::PeerDiscovery(PeerDiscoveryMsg::Hello(peer)) => peer.node_id,
                _ => 0, // TODO: Handle other message types
            };

            // Send the message to the node
            if let Err(e) = message_sender.send((sender_id, msg)).await {
                error!("Failed to send message to node: {}", e);
                return Err(HotStuffError::Network(format!(
                    "Failed to send message to node: {}",
                    e
                )));
            }
        }

        Ok(())
    }
}
