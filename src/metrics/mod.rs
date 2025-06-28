use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Metrics collected by the HotStuff-2 implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusMetrics {
    // Basic counters
    pub blocks_proposed: u64,
    pub blocks_committed: u64,
    pub votes_sent: u64,
    pub votes_received: u64,
    pub timeouts: u64,
    pub view_changes: u64,
    
    // Performance metrics
    pub avg_block_time: f64,        // Average time between blocks (ms)
    pub avg_commit_latency: f64,    // Average time from proposal to commit (ms)
    pub current_height: u64,
    pub current_round: u64,
    
    // Safety metrics
    pub safety_violations: u64,
    pub consecutive_timeouts: u64,
    pub max_consecutive_timeouts: u64,
    
    // Network metrics
    pub messages_sent: u64,
    pub messages_received: u64,
    pub network_errors: u64,
    
    // Storage metrics
    pub blocks_stored: u64,
    pub storage_errors: u64,
    
    // Timestamp
    pub timestamp: u64,
}

impl Default for ConsensusMetrics {
    fn default() -> Self {
        Self {
            blocks_proposed: 0,
            blocks_committed: 0,
            votes_sent: 0,
            votes_received: 0,
            timeouts: 0,
            view_changes: 0,
            avg_block_time: 0.0,
            avg_commit_latency: 0.0,
            current_height: 0,
            current_round: 0,
            safety_violations: 0,
            consecutive_timeouts: 0,
            max_consecutive_timeouts: 0,
            messages_sent: 0,
            messages_received: 0,
            network_errors: 0,
            blocks_stored: 0,
            storage_errors: 0,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        }
    }
}

/// Events that can be recorded by the metrics system
#[derive(Debug, Clone)]
pub enum MetricEvent {
    BlockProposed { height: u64, timestamp: Instant },
    BlockCommitted { height: u64, timestamp: Instant },
    VoteSent { round: u64 },
    VoteReceived { round: u64 },
    Timeout { round: u64 },
    ViewChange { old_round: u64, new_round: u64 },
    SafetyViolation { violation_type: String },
    MessageSent { peer_id: u64, message_type: String },
    MessageReceived { peer_id: u64, message_type: String },
    NetworkError { error: String },
    StorageError { error: String },
    Custom { name: String, value: f64, timestamp: Instant },
}

/// Metrics collector for HotStuff-2
pub struct MetricsCollector {
    metrics: Arc<RwLock<ConsensusMetrics>>,
    event_sender: mpsc::Sender<MetricEvent>,
    event_receiver: Option<mpsc::Receiver<MetricEvent>>,
    
    // Internal state for computing averages
    block_times: Arc<RwLock<Vec<Duration>>>,
    commit_latencies: Arc<RwLock<HashMap<u64, Instant>>>, // height -> proposal_time
    
    // Atomic counters for high-frequency events
    blocks_proposed: Arc<AtomicU64>,
    blocks_committed: Arc<AtomicU64>,
    votes_sent: Arc<AtomicU64>,
    votes_received: Arc<AtomicU64>,
    messages_sent: Arc<AtomicU64>,
    messages_received: Arc<AtomicU64>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let (event_sender, event_receiver) = mpsc::channel(1000);
        
        Self {
            metrics: Arc::new(RwLock::new(ConsensusMetrics::default())),
            event_sender,
            event_receiver: Some(event_receiver),
            block_times: Arc::new(RwLock::new(Vec::new())),
            commit_latencies: Arc::new(RwLock::new(HashMap::new())),
            blocks_proposed: Arc::new(AtomicU64::new(0)),
            blocks_committed: Arc::new(AtomicU64::new(0)),
            votes_sent: Arc::new(AtomicU64::new(0)),
            votes_received: Arc::new(AtomicU64::new(0)),
            messages_sent: Arc::new(AtomicU64::new(0)),
            messages_received: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get the event sender for recording metrics
    pub fn event_sender(&self) -> mpsc::Sender<MetricEvent> {
        self.event_sender.clone()
    }

    /// Start the metrics collection loop
    pub async fn start(&mut self) {
        if let Some(mut receiver) = self.event_receiver.take() {
            let metrics = self.metrics.clone();
            let block_times = self.block_times.clone();
            let commit_latencies = self.commit_latencies.clone();
            let blocks_proposed = Arc::clone(&self.blocks_proposed);
            let blocks_committed = Arc::clone(&self.blocks_committed);
            let votes_sent = Arc::clone(&self.votes_sent);
            let votes_received = Arc::clone(&self.votes_received);
            let messages_sent = Arc::clone(&self.messages_sent);
            let messages_received = Arc::clone(&self.messages_received);
            
            tokio::spawn(async move {
                let mut last_block_time = Instant::now();
                
                while let Some(event) = receiver.recv().await {
                    match event {
                        MetricEvent::BlockProposed { height, timestamp } => {
                            blocks_proposed.fetch_add(1, Ordering::Relaxed);
                            
                            // Record block time
                            let block_time = timestamp.duration_since(last_block_time);
                            last_block_time = timestamp;
                            
                            if let Ok(mut times) = block_times.write() {
                                times.push(block_time);
                                if times.len() > 100 {
                                    times.remove(0);
                                }
                            }
                            
                            // Record proposal time for latency calculation
                            if let Ok(mut latencies) = commit_latencies.write() {
                                latencies.insert(height, timestamp);
                            }
                            
                            // Update metrics
                            if let Ok(mut m) = metrics.write() {
                                m.current_height = height;
                                m.blocks_proposed = blocks_proposed.load(Ordering::Relaxed);
                            }
                        }
                        
                        MetricEvent::BlockCommitted { height, timestamp } => {
                            blocks_committed.fetch_add(1, Ordering::Relaxed);
                            
                            // Calculate commit latency
                            if let Ok(mut latencies) = commit_latencies.write() {
                                if let Some(proposal_time) = latencies.remove(&height) {
                                    let commit_latency = timestamp.duration_since(proposal_time);
                                    
                                    if let Ok(mut m) = metrics.write() {
                                        // Update average commit latency (simple moving average)
                                        let new_latency = commit_latency.as_millis() as f64;
                                        if m.avg_commit_latency == 0.0 {
                                            m.avg_commit_latency = new_latency;
                                        } else {
                                            m.avg_commit_latency = (m.avg_commit_latency * 0.9) + (new_latency * 0.1);
                                        }
                                    }
                                }
                            }
                            
                            if let Ok(mut m) = metrics.write() {
                                m.blocks_committed = blocks_committed.load(Ordering::Relaxed);
                            }
                        }
                        
                        MetricEvent::VoteSent { round } => {
                            votes_sent.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut m) = metrics.write() {
                                m.votes_sent = votes_sent.load(Ordering::Relaxed);
                                m.current_round = round;
                            }
                        }
                        
                        MetricEvent::VoteReceived { round: _ } => {
                            votes_received.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut m) = metrics.write() {
                                m.votes_received = votes_received.load(Ordering::Relaxed);
                            }
                        }
                        
                        MetricEvent::Timeout { round: _ } => {
                            if let Ok(mut m) = metrics.write() {
                                m.timeouts += 1;
                                m.consecutive_timeouts += 1;
                                m.max_consecutive_timeouts = m.max_consecutive_timeouts.max(m.consecutive_timeouts);
                            }
                        }
                        
                        MetricEvent::ViewChange { old_round: _, new_round } => {
                            if let Ok(mut m) = metrics.write() {
                                m.view_changes += 1;
                                m.current_round = new_round;
                                m.consecutive_timeouts = 0; // Reset on successful view change
                            }
                        }
                        
                        MetricEvent::SafetyViolation { violation_type: _ } => {
                            if let Ok(mut m) = metrics.write() {
                                m.safety_violations += 1;
                            }
                        }
                        
                        MetricEvent::MessageSent { peer_id: _, message_type: _ } => {
                            messages_sent.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut m) = metrics.write() {
                                m.messages_sent = messages_sent.load(Ordering::Relaxed);
                            }
                        }
                        
                        MetricEvent::MessageReceived { peer_id: _, message_type: _ } => {
                            messages_received.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut m) = metrics.write() {
                                m.messages_received = messages_received.load(Ordering::Relaxed);
                            }
                        }
                        
                        MetricEvent::NetworkError { error: _ } => {
                            if let Ok(mut m) = metrics.write() {
                                m.network_errors += 1;
                            }
                        }
                        
                        MetricEvent::StorageError { error: _ } => {
                            if let Ok(mut m) = metrics.write() {
                                m.storage_errors += 1;
                            }
                        }
                        
                        MetricEvent::Custom { name: _, value: _, timestamp: _ } => {
                            // Custom metrics can be handled specifically if needed
                            // For now, just log the custom event
                        }
                    }
                    
                    // Update average block time
                    if let Ok(times) = block_times.read() {
                        if !times.is_empty() {
                            let avg_time = times.iter().map(|d| d.as_millis() as f64).sum::<f64>() / times.len() as f64;
                            if let Ok(mut m) = metrics.write() {
                                m.avg_block_time = avg_time;
                            }
                        }
                    }
                    
                    // Update timestamp
                    if let Ok(mut m) = metrics.write() {
                        m.timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                    }
                }
            });
        }
    }

    /// Get current metrics snapshot
    pub fn get_metrics(&self) -> ConsensusMetrics {
        self.metrics.read().unwrap().clone()
    }

    /// Reset all metrics
    pub fn reset(&self) {
        if let Ok(mut m) = self.metrics.write() {
            *m = ConsensusMetrics::default();
        }
        
        self.blocks_proposed.store(0, Ordering::Relaxed);
        self.blocks_committed.store(0, Ordering::Relaxed);
        self.votes_sent.store(0, Ordering::Relaxed);
        self.votes_received.store(0, Ordering::Relaxed);
        self.messages_sent.store(0, Ordering::Relaxed);
        self.messages_received.store(0, Ordering::Relaxed);
        
        if let Ok(mut times) = self.block_times.write() {
            times.clear();
        }
        
        if let Ok(mut latencies) = self.commit_latencies.write() {
            latencies.clear();
        }
    }
}

/// Simple HTTP metrics server
pub struct MetricsServer {
    collector: Arc<MetricsCollector>,
    port: u16,
}

impl MetricsServer {
    pub fn new(collector: Arc<MetricsCollector>, port: u16) -> Self {
        Self { collector, port }
    }

    /// Start the metrics HTTP server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use warp::Filter;
        
        let collector = self.collector.clone();
        let metrics_route = warp::path("metrics")
            .map(move || {
                let metrics = collector.get_metrics();
                warp::reply::json(&metrics)
            });
        
        let health_route = warp::path("health")
            .map(|| "OK");
        
        let routes = metrics_route.or(health_route);
        
        println!("Starting metrics server on port {}", self.port);
        warp::serve(routes)
            .run(([127, 0, 0, 1], self.port))
            .await;
        
        Ok(())
    }
}
