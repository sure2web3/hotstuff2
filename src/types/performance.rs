use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStatistics {
    pub current_height: u64,
    pub current_view: u64,
    pub pending_transactions: usize,
    pub is_synchronous: bool,
    pub fast_path_enabled: bool,
    pub pipeline_stages: usize,
    pub last_commit_time: SystemTime,
    pub throughput_tps: f64,
    pub latency_ms: f64,
    pub network_conditions: NetworkConditions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConditions {
    pub is_synchronous: bool,
    pub confidence: f64,
    pub estimated_delay_ms: u64,
}
