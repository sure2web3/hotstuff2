/// HotStuff-2 Recovery and Fault Tolerance System
/// 
/// This module provides comprehensive recovery mechanisms for the HotStuff-2 consensus protocol
/// including state recovery, view change recovery, and Byzantine fault tolerance.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use log::{error, info, warn};

use crate::consensus::state_machine::{StateSnapshot, StateMachine};
use crate::error::HotStuffError;
use crate::protocol::hotstuff2::{ChainState, HotStuff2};
use crate::storage::BlockStore;
use crate::types::{Block, Hash, QuorumCert};

/// Recovery manager for handling various failure scenarios
pub struct RecoveryManager<B: BlockStore + ?Sized + 'static> {
    node: Arc<HotStuff2<B>>,
    checkpoints: RwLock<VecDeque<Checkpoint>>,
    max_checkpoints: usize,
    recovery_stats: Mutex<RecoveryStats>,
}

/// Checkpoint containing state snapshot for recovery
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub height: u64,
    pub view: u64,
    pub chain_state: ChainState,
    pub state_snapshot: StateSnapshot,
    pub timestamp: Instant,
    pub block_hashes: Vec<Hash>,
}

/// Recovery statistics for monitoring
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    pub total_recoveries: u64,
    pub view_change_recoveries: u64,
    pub state_recoveries: u64,
    pub checkpoint_recoveries: u64,
    pub last_recovery_time: Option<Instant>,
    pub last_recovery_duration: Option<Duration>,
}

/// Types of recovery operations
#[derive(Debug, Clone, Copy)]
pub enum RecoveryType {
    ViewChange,
    StateCorruption,
    NetworkPartition,
    ByzantineFault,
    CheckpointRestore,
}

impl<B: BlockStore + ?Sized + 'static> RecoveryManager<B> {
    pub fn new(node: Arc<HotStuff2<B>>) -> Self {
        Self {
            node,
            checkpoints: RwLock::new(VecDeque::new()),
            max_checkpoints: 10,
            recovery_stats: Mutex::new(RecoveryStats::default()),
        }
    }

    /// Create a checkpoint of the current state
    pub async fn create_checkpoint(&self) -> Result<(), HotStuffError> {
        info!("Creating recovery checkpoint");

        let chain_state = self.node.chain_state.lock().await.clone();
        let view = self.node.current_view.lock().await;
        let current_view = view.number;
        let current_height = chain_state.committed_height;
        drop(view);

        // Get state machine snapshot
        let state_machine = self.node.state_machine.lock().await;
        let state_snapshot = if let Ok(kv_state) = state_machine.downcast_ref::<crate::consensus::state_machine::KVStateMachine>() {
            kv_state.create_snapshot()
        } else {
            return Err(HotStuffError::Storage("Failed to create state snapshot".to_string()));
        };
        drop(state_machine);

        // Collect recent block hashes
        let mut block_hashes = Vec::new();
        let start_height = current_height.saturating_sub(10);
        for height in start_height..=current_height {
            if let Some(qc) = &chain_state.high_qc {
                if qc.height >= height {
                    block_hashes.push(qc.block_hash);
                }
            }
        }

        let checkpoint = Checkpoint {
            height: current_height,
            view: current_view,
            chain_state,
            state_snapshot,
            timestamp: Instant::now(),
            block_hashes,
        };

        let mut checkpoints = self.checkpoints.write().await;
        checkpoints.push_back(checkpoint);

        // Maintain maximum number of checkpoints
        while checkpoints.len() > self.max_checkpoints {
            checkpoints.pop_front();
        }

        info!("Checkpoint created at height {} view {}", current_height, current_view);
        Ok(())
    }

    /// Recover from the most recent checkpoint
    pub async fn recover_from_checkpoint(&self) -> Result<(), HotStuffError> {
        let start_time = Instant::now();
        info!("Starting recovery from checkpoint");

        let checkpoints = self.checkpoints.read().await;
        let checkpoint = checkpoints.back()
            .ok_or_else(|| HotStuffError::Storage("No checkpoints available for recovery".to_string()))?
            .clone();
        drop(checkpoints);

        // Restore chain state
        {
            let mut chain_state = self.node.chain_state.lock().await;
            *chain_state = checkpoint.chain_state.clone();
        }

        // Restore view
        {
            let mut view = self.node.current_view.lock().await;
            view.number = checkpoint.view;
        }

        // Restore state machine
        {
            let mut state_machine = self.node.state_machine.lock().await;
            if let Ok(kv_state) = state_machine.downcast_mut::<crate::consensus::state_machine::KVStateMachine>() {
                kv_state.restore_from_snapshot(checkpoint.state_snapshot);
            }
        }

        // Update recovery stats
        let recovery_duration = start_time.elapsed();
        {
            let mut stats = self.recovery_stats.lock().await;
            stats.total_recoveries += 1;
            stats.checkpoint_recoveries += 1;
            stats.last_recovery_time = Some(start_time);
            stats.last_recovery_duration = Some(recovery_duration);
        }

        info!("Recovery completed in {:?}, restored to height {} view {}", 
              recovery_duration, checkpoint.height, checkpoint.view);
        Ok(())
    }

    /// Handle view change recovery
    pub async fn recover_from_view_change(&self, target_view: u64) -> Result<(), HotStuffError> {
        let start_time = Instant::now();
        info!("Recovering from view change to view {}", target_view);

        // Update view
        {
            let mut view = self.node.current_view.lock().await;
            if target_view > view.number {
                view.number = target_view;
                view.start_time = Instant::now();
                
                // Update leader for new view
                let leader_election = self.node.leader_election.read();
                view.leader = leader_election.get_leader(target_view);
            }
        }

        // Clear any invalid pipeline stages
        self.node.pipeline.retain(|_, stage| stage.view <= target_view);

        // Update recovery stats
        let recovery_duration = start_time.elapsed();
        {
            let mut stats = self.recovery_stats.lock().await;
            stats.total_recoveries += 1;
            stats.view_change_recoveries += 1;
            stats.last_recovery_time = Some(start_time);
            stats.last_recovery_duration = Some(recovery_duration);
        }

        info!("View change recovery completed in {:?}", recovery_duration);
        Ok(())
    }

    /// Handle Byzantine fault detection and recovery
    pub async fn handle_byzantine_fault(&self, suspected_node: u64, evidence: String) -> Result<(), HotStuffError> {
        warn!("Byzantine fault detected from node {}: {}", suspected_node, evidence);

        // In a production system, this would:
        // 1. Report the fault to other nodes
        // 2. Exclude the Byzantine node from future consensus
        // 3. Trigger view change if necessary
        // 4. Update fault tolerance thresholds

        // For now, just trigger a view change
        let current_view = self.node.current_view.lock().await.number;
        self.recover_from_view_change(current_view + 1).await?;

        info!("Byzantine fault recovery completed");
        Ok(())
    }

    /// Handle network partition recovery
    pub async fn handle_network_partition_recovery(&self) -> Result<(), HotStuffError> {
        info!("Handling network partition recovery");

        // Reset synchrony detector
        {
            let mut detector = self.node.synchrony_detector.lock().await;
            detector.recent_timings.clear();
            detector.is_synchronous = false;
        }

        // Clear timeouts and restart pacemaker
        self.node.timeouts.clear();

        // Trigger view change to re-establish consensus
        let current_view = self.node.current_view.lock().await.number;
        self.recover_from_view_change(current_view + 1).await?;

        info!("Network partition recovery completed");
        Ok(())
    }

    /// Comprehensive health check
    pub async fn health_check(&self) -> Result<HealthStatus, HotStuffError> {
        let chain_state = self.node.chain_state.lock().await;
        let view = self.node.current_view.lock().await;
        let detector = self.node.synchrony_detector.lock().await;
        let stats = self.recovery_stats.lock().await;

        let health = HealthStatus {
            is_healthy: self.is_node_healthy(&chain_state, &view, &detector).await,
            current_height: chain_state.committed_height,
            current_view: view.number,
            is_synchronous: detector.is_synchronous,
            pipeline_stages: self.node.pipeline.len(),
            pending_transactions: self.node.transaction_pool.lock().await.len(),
            recovery_stats: stats.clone(),
            has_lock: chain_state.locked_qc.is_some(),
            last_activity: view.start_time,
        };

        Ok(health)
    }

    /// Check if node is healthy
    async fn is_node_healthy(
        &self,
        chain_state: &ChainState,
        view: &crate::protocol::hotstuff2::View,
        detector: &crate::protocol::hotstuff2::SynchronyDetector,
    ) -> bool {
        // Node is healthy if:
        // 1. It has a valid chain state
        // 2. View is progressing (not stuck)
        // 3. Network is responsive
        // 4. No excessive pipeline congestion

        let view_age = view.start_time.elapsed();
        let max_view_age = Duration::from_secs(30);
        
        let pipeline_ok = self.node.pipeline.len() < 100; // Not too congested
        let view_ok = view_age < max_view_age; // View not stuck
        let network_ok = detector.recent_timings.len() > 0; // Network activity

        pipeline_ok && view_ok && network_ok
    }

    /// Get recovery statistics
    pub async fn get_recovery_stats(&self) -> RecoveryStats {
        self.recovery_stats.lock().await.clone()
    }

    /// Automatic recovery based on health status
    pub async fn auto_recovery(&self) -> Result<(), HotStuffError> {
        let health = self.health_check().await?;

        if !health.is_healthy {
            warn!("Node unhealthy, attempting automatic recovery");

            // Choose recovery strategy based on the issue
            if health.pipeline_stages > 50 {
                // Too many pipeline stages - clear old ones
                self.clear_stale_pipeline_stages().await?;
            }

            if health.last_activity.elapsed() > Duration::from_secs(30) {
                // View stuck - trigger view change
                self.recover_from_view_change(health.current_view + 1).await?;
            }

            if !health.is_synchronous && health.recovery_stats.total_recoveries > 0 {
                // Network issues - reset synchrony detection
                self.handle_network_partition_recovery().await?;
            }
        }

        Ok(())
    }

    /// Clear stale pipeline stages
    async fn clear_stale_pipeline_stages(&self) -> Result<(), HotStuffError> {
        let cutoff_time = Instant::now() - Duration::from_secs(60);
        let current_view = self.node.current_view.lock().await.number;

        self.node.pipeline.retain(|_, stage| {
            stage.start_time > cutoff_time && stage.view >= current_view.saturating_sub(5)
        });

        info!("Cleared stale pipeline stages");
        Ok(())
    }
}

/// Health status of the consensus node
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub current_height: u64,
    pub current_view: u64,
    pub is_synchronous: bool,
    pub pipeline_stages: usize,
    pub pending_transactions: usize,
    pub recovery_stats: RecoveryStats,
    pub has_lock: bool,
    pub last_activity: Instant,
}

impl Default for RecoveryStats {
    fn default() -> Self {
        Self {
            total_recoveries: 0,
            view_change_recoveries: 0,
            state_recoveries: 0,
            checkpoint_recoveries: 0,
            last_recovery_time: None,
            last_recovery_duration: None,
        }
    }
}

/// Recovery configuration
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    pub checkpoint_interval: Duration,
    pub max_checkpoints: usize,
    pub auto_recovery_enabled: bool,
    pub health_check_interval: Duration,
    pub view_timeout: Duration,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            checkpoint_interval: Duration::from_secs(30),
            max_checkpoints: 10,
            auto_recovery_enabled: true,
            health_check_interval: Duration::from_secs(10),
            view_timeout: Duration::from_secs(30),
        }
    }
}
