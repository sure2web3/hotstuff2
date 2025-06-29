// Network reliability and fault tolerance for HotStuff-2
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;

use crate::error::HotStuffError;
use crate::network::{P2PMessage, MessagePayload};

/// Message delivery guarantees
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryGuarantee {
    BestEffort,        // Fire and forget
    AtLeastOnce,       // With retries
    ExactlyOnce,       // With deduplication
}

/// Network reliability manager
pub struct NetworkReliabilityManager {
    node_id: u64,
    
    // Message tracking
    message_history: Arc<RwLock<HashMap<u64, MessageRecord>>>,
    delivery_queue: Arc<Mutex<VecDeque<RetryableMessage>>>,
    acknowledgments: Arc<RwLock<HashMap<u64, Instant>>>,
    
    // Configuration
    retry_attempts: u32,
    retry_backoff: Duration,
    ack_timeout: Duration,
    history_retention: Duration,
}

#[derive(Debug, Clone)]
struct MessageRecord {
    id: u64,
    from: u64,
    to: Option<u64>,
    timestamp: Instant,
    attempts: u32,
    last_attempt: Option<Instant>,
    delivery_guarantee: DeliveryGuarantee,
    payload_hash: u64, // For deduplication
}

#[derive(Debug, Clone)]
struct RetryableMessage {
    message: P2PMessage,
    delivery_guarantee: DeliveryGuarantee,
    attempts: u32,
    next_retry: Instant,
    max_attempts: u32,
}

impl NetworkReliabilityManager {
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            message_history: Arc::new(RwLock::new(HashMap::new())),
            delivery_queue: Arc::new(Mutex::new(VecDeque::new())),
            acknowledgments: Arc::new(RwLock::new(HashMap::new())),
            retry_attempts: 5,
            retry_backoff: Duration::from_millis(500),
            ack_timeout: Duration::from_secs(10),
            history_retention: Duration::from_secs(300), // 5 minutes
        }
    }
    
    /// Start reliability services
    pub async fn start(&self) -> Result<(), HotStuffError> {
        info!("Starting network reliability manager for node {}", self.node_id);
        
        self.start_retry_processor().await?;
        self.start_acknowledgment_tracker().await?;
        self.start_cleanup_service().await?;
        
        Ok(())
    }
    
    /// Send message with specified delivery guarantee
    pub async fn send_reliable(
        &self,
        message: P2PMessage,
        guarantee: DeliveryGuarantee,
        sender_fn: impl Fn(P2PMessage) -> Result<(), HotStuffError> + Send + 'static,
    ) -> Result<(), HotStuffError> {
        let payload_hash = self.calculate_payload_hash(&message.payload);
        
        // Check for duplicate (exactly-once semantics)
        if guarantee == DeliveryGuarantee::ExactlyOnce {
            let history = self.message_history.read().await;
            if let Some(record) = history.get(&message.id) {
                if record.payload_hash == payload_hash {
                    debug!("Duplicate message {} detected, skipping", message.id);
                    return Ok(());
                }
            }
        }
        
        // Record message
        let record = MessageRecord {
            id: message.id,
            from: message.from,
            to: Some(message.to),
            timestamp: Instant::now(),
            attempts: 1,
            last_attempt: Some(Instant::now()),
            delivery_guarantee: guarantee,
            payload_hash,
        };
        
        {
            let mut history = self.message_history.write().await;
            history.insert(message.id, record);
        }
        
        // Send message
        match sender_fn(message.clone()) {
            Ok(()) => {
                debug!("Message {} sent successfully", message.id);
                
                // For best-effort, we're done
                if guarantee == DeliveryGuarantee::BestEffort {
                    return Ok(());
                }
                
                // For guaranteed delivery, add to retry queue
                let retryable = RetryableMessage {
                    message,
                    delivery_guarantee: guarantee,
                    attempts: 1,
                    next_retry: Instant::now() + self.retry_backoff,
                    max_attempts: self.retry_attempts,
                };
                
                self.delivery_queue.lock().await.push_back(retryable);
            }
            Err(e) => {
                error!("Failed to send message {}: {}", message.id, e);
                
                // Add to retry queue regardless of guarantee
                let retryable = RetryableMessage {
                    message,
                    delivery_guarantee: guarantee,
                    attempts: 0,
                    next_retry: Instant::now() + self.retry_backoff,
                    max_attempts: self.retry_attempts,
                };
                
                self.delivery_queue.lock().await.push_back(retryable);
            }
        }
        
        Ok(())
    }
    
    /// Process acknowledgment
    pub async fn process_acknowledgment(&self, message_id: u64) -> Result<(), HotStuffError> {
        debug!("Processing acknowledgment for message {}", message_id);
        
        // Record acknowledgment
        {
            let mut acks = self.acknowledgments.write().await;
            acks.insert(message_id, Instant::now());
        }
        
        // Remove from retry queue
        {
            let mut queue = self.delivery_queue.lock().await;
            queue.retain(|msg| msg.message.id != message_id);
        }
        
        // Update message history
        {
            let mut history = self.message_history.write().await;
            if let Some(record) = history.get_mut(&message_id) {
                record.last_attempt = Some(Instant::now());
            }
        }
        
        Ok(())
    }
    
    /// Check if message is duplicate
    pub async fn is_duplicate(&self, message: &P2PMessage) -> bool {
        let payload_hash = self.calculate_payload_hash(&message.payload);
        
        let history = self.message_history.read().await;
        if let Some(record) = history.get(&message.id) {
            record.payload_hash == payload_hash
        } else {
            false
        }
    }
    
    /// Get reliability statistics
    pub async fn get_reliability_stats(&self) -> ReliabilityStats {
        let history = self.message_history.read().await;
        let queue = self.delivery_queue.lock().await;
        let acks = self.acknowledgments.read().await;
        
        let total_messages = history.len();
        let acknowledged = acks.len();
        let pending_retries = queue.len();
        
        let failed_messages = history.values()
            .filter(|record| record.attempts >= self.retry_attempts)
            .count();
        
        let success_rate = if total_messages > 0 {
            (acknowledged as f64 / total_messages as f64) * 100.0
        } else {
            0.0
        };
        
        ReliabilityStats {
            node_id: self.node_id,
            total_messages,
            acknowledged_messages: acknowledged,
            pending_retries,
            failed_messages,
            success_rate,
        }
    }
    
    async fn start_retry_processor(&self) -> Result<(), HotStuffError> {
        let delivery_queue = Arc::clone(&self.delivery_queue);
        let message_history = Arc::clone(&self.message_history);
        let retry_backoff = self.retry_backoff;
        
        tokio::spawn(async move {
            let mut retry_interval = interval(Duration::from_millis(100));
            
            loop {
                retry_interval.tick().await;
                
                let now = Instant::now();
                let mut messages_to_retry = Vec::new();
                
                // Find messages ready for retry
                {
                    let mut queue = delivery_queue.lock().await;
                    while let Some(msg) = queue.front() {
                        if msg.next_retry <= now {
                            messages_to_retry.push(queue.pop_front().unwrap());
                        } else {
                            break;
                        }
                    }
                }
                
                // Process retries
                for mut retryable in messages_to_retry {
                    if retryable.attempts >= retryable.max_attempts {
                        error!("Message {} failed after {} attempts", 
                               retryable.message.id, retryable.attempts);
                        continue;
                    }
                    
                    retryable.attempts += 1;
                    retryable.next_retry = now + retry_backoff * retryable.attempts;
                    
                    debug!("Retrying message {} (attempt {})", 
                           retryable.message.id, retryable.attempts);
                    
                    // Update history
                    {
                        let mut history = message_history.write().await;
                        if let Some(record) = history.get_mut(&retryable.message.id) {
                            record.attempts = retryable.attempts;
                            record.last_attempt = Some(now);
                        }
                    }
                    
                    // Re-queue for retry
                    delivery_queue.lock().await.push_back(retryable);
                }
            }
        });
        
        Ok(())
    }
    
    async fn start_acknowledgment_tracker(&self) -> Result<(), HotStuffError> {
        let _acknowledgments = Arc::clone(&self.acknowledgments);
        let delivery_queue = Arc::clone(&self.delivery_queue);
        let ack_timeout = self.ack_timeout;
        
        tokio::spawn(async move {
            let mut ack_interval = interval(Duration::from_secs(5));
            
            loop {
                ack_interval.tick().await;
                
                let now = Instant::now();
                
                // Find timed-out acknowledgments
                let timed_out_messages: Vec<u64> = {
                    let queue = delivery_queue.lock().await;
                    queue.iter()
                        .filter(|msg| {
                            msg.delivery_guarantee != DeliveryGuarantee::BestEffort &&
                            now.duration_since(msg.next_retry) > ack_timeout
                        })
                        .map(|msg| msg.message.id)
                        .collect()
                };
                
                for message_id in timed_out_messages {
                    warn!("Acknowledgment timeout for message {}", message_id);
                    // Could trigger additional retry or failure handling here
                }
            }
        });
        
        Ok(())
    }
    
    async fn start_cleanup_service(&self) -> Result<(), HotStuffError> {
        let message_history = Arc::clone(&self.message_history);
        let acknowledgments = Arc::clone(&self.acknowledgments);
        let retention_period = self.history_retention;
        
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(60)); // 1 minute
            
            loop {
                cleanup_interval.tick().await;
                
                let now = Instant::now();
                
                // Clean up old message history
                {
                    let mut history = message_history.write().await;
                    history.retain(|_, record| {
                        now.duration_since(record.timestamp) < retention_period
                    });
                }
                
                // Clean up old acknowledgments
                {
                    let mut acks = acknowledgments.write().await;
                    acks.retain(|_, timestamp| {
                        now.duration_since(*timestamp) < retention_period
                    });
                }
            }
        });
        
        Ok(())
    }
    
    fn calculate_payload_hash(&self, payload: &MessagePayload) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash the payload for deduplication
        match payload {
            MessagePayload::Consensus(msg) => {
                "consensus".hash(&mut hasher);
                // In a real implementation, hash the actual message content
                format!("{:?}", msg).hash(&mut hasher);
            }
            MessagePayload::Network(msg) => {
                "network".hash(&mut hasher);
                format!("{:?}", msg).hash(&mut hasher);
            }
            MessagePayload::Heartbeat => {
                "heartbeat".hash(&mut hasher);
            }
            MessagePayload::Acknowledgment { ack_id } => {
                "acknowledgment".hash(&mut hasher);
                ack_id.hash(&mut hasher);
            }
        }
        
        hasher.finish()
    }
}

/// Network fault detector and handler
pub struct NetworkFaultDetector {
    node_id: u64,
    peer_health: Arc<RwLock<HashMap<u64, PeerHealthRecord>>>,
    failure_thresholds: FaultDetectionThresholds,
}

#[derive(Debug, Clone)]
struct PeerHealthRecord {
    node_id: u64,
    last_message: Instant,
    consecutive_failures: u32,
    total_messages: u64,
    failed_messages: u64,
    average_latency: Duration,
    is_suspected_faulty: bool,
}

#[derive(Debug, Clone)]
pub struct FaultDetectionThresholds {
    pub max_consecutive_failures: u32,
    pub failure_rate_threshold: f64,   // 0.0 to 1.0
    pub message_timeout: Duration,
    pub peer_timeout: Duration,
}

impl Default for FaultDetectionThresholds {
    fn default() -> Self {
        Self {
            max_consecutive_failures: 5,
            failure_rate_threshold: 0.3,
            message_timeout: Duration::from_secs(10),
            peer_timeout: Duration::from_secs(30),
        }
    }
}

impl NetworkFaultDetector {
    pub fn new(node_id: u64, thresholds: FaultDetectionThresholds) -> Self {
        Self {
            node_id,
            peer_health: Arc::new(RwLock::new(HashMap::new())),
            failure_thresholds: thresholds,
        }
    }
    
    /// Start fault detection service
    pub async fn start(&self) -> Result<(), HotStuffError> {
        self.start_peer_monitoring().await?;
        Ok(())
    }
    
    /// Record successful message from peer
    pub async fn record_peer_message(&self, peer_id: u64, latency: Duration) {
        let mut health = self.peer_health.write().await;
        let record = health.entry(peer_id).or_insert_with(|| PeerHealthRecord {
            node_id: peer_id,
            last_message: Instant::now(),
            consecutive_failures: 0,
            total_messages: 0,
            failed_messages: 0,
            average_latency: Duration::from_millis(0),
            is_suspected_faulty: false,
        });
        
        record.last_message = Instant::now();
        record.consecutive_failures = 0;
        record.total_messages += 1;
        record.is_suspected_faulty = false;
        
        // Update average latency (simple moving average)
        record.average_latency = Duration::from_millis(
            ((record.average_latency.as_millis() as u64 + latency.as_millis() as u64) / 2) as u64
        );
    }
    
    /// Record failed message to peer
    pub async fn record_peer_failure(&self, peer_id: u64) {
        let mut health = self.peer_health.write().await;
        let record = health.entry(peer_id).or_insert_with(|| PeerHealthRecord {
            node_id: peer_id,
            last_message: Instant::now(),
            consecutive_failures: 0,
            total_messages: 0,
            failed_messages: 0,
            average_latency: Duration::from_millis(100),
            is_suspected_faulty: false,
        });
        
        record.consecutive_failures += 1;
        record.failed_messages += 1;
        record.total_messages += 1;
        
        // Check if peer should be marked as faulty
        let failure_rate = record.failed_messages as f64 / record.total_messages as f64;
        
        if record.consecutive_failures >= self.failure_thresholds.max_consecutive_failures ||
           failure_rate > self.failure_thresholds.failure_rate_threshold {
            record.is_suspected_faulty = true;
            warn!("Peer {} marked as suspected faulty (failures: {}, rate: {:.2})", 
                  peer_id, record.consecutive_failures, failure_rate);
        }
    }
    
    /// Check if peer is suspected faulty
    pub async fn is_peer_faulty(&self, peer_id: u64) -> bool {
        let health = self.peer_health.read().await;
        health.get(&peer_id)
            .map(|record| record.is_suspected_faulty)
            .unwrap_or(false)
    }
    
    /// Get fault detection statistics
    pub async fn get_fault_stats(&self) -> FaultDetectionStats {
        let health = self.peer_health.read().await;
        
        let total_peers = health.len();
        let faulty_peers = health.values().filter(|r| r.is_suspected_faulty).count();
        let healthy_peers = total_peers - faulty_peers;
        
        let avg_failure_rate = if total_peers > 0 {
            health.values()
                .map(|r| r.failed_messages as f64 / r.total_messages.max(1) as f64)
                .sum::<f64>() / total_peers as f64
        } else {
            0.0
        };
        
        FaultDetectionStats {
            node_id: self.node_id,
            total_peers,
            healthy_peers,
            faulty_peers,
            average_failure_rate: avg_failure_rate,
        }
    }
    
    async fn start_peer_monitoring(&self) -> Result<(), HotStuffError> {
        let peer_health = Arc::clone(&self.peer_health);
        let peer_timeout = self.failure_thresholds.peer_timeout;
        
        tokio::spawn(async move {
            let mut monitor_interval = interval(Duration::from_secs(10));
            
            loop {
                monitor_interval.tick().await;
                
                let now = Instant::now();
                
                // Check for timed-out peers
                {
                    let mut health = peer_health.write().await;
                    for record in health.values_mut() {
                        if now.duration_since(record.last_message) > peer_timeout {
                            if !record.is_suspected_faulty {
                                warn!("Peer {} timed out", record.node_id);
                                record.is_suspected_faulty = true;
                                record.consecutive_failures += 1;
                            }
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ReliabilityStats {
    pub node_id: u64,
    pub total_messages: usize,
    pub acknowledged_messages: usize,
    pub pending_retries: usize,
    pub failed_messages: usize,
    pub success_rate: f64,
}

#[derive(Debug, Clone)]
pub struct FaultDetectionStats {
    pub node_id: u64,
    pub total_peers: usize,
    pub healthy_peers: usize,
    pub faulty_peers: usize,
    pub average_failure_rate: f64,
}

impl std::fmt::Display for ReliabilityStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node {}: {}/{} msgs acked ({:.1}% success), {} pending, {} failed",
            self.node_id,
            self.acknowledged_messages,
            self.total_messages,
            self.success_rate,
            self.pending_retries,
            self.failed_messages
        )
    }
}

impl std::fmt::Display for FaultDetectionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node {}: {}/{} peers healthy, {} faulty, {:.2}% avg failure rate",
            self.node_id,
            self.healthy_peers,
            self.total_peers,
            self.faulty_peers,
            self.average_failure_rate * 100.0
        )
    }
}
