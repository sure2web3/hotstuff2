use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use crate::error::HotStuffError;
use crate::types::QuorumCert;

/// Events that the pacemaker can emit
#[derive(Debug, Clone)]
pub enum PacemakerEvent {
    LocalTimeout(u64), // round
    NewRound(u64),     // new round number
    NewView(u64),      // new view number
}

/// Manages timing and round progression for HotStuff-2
pub struct Pacemaker {
    current_round: u64,
    round_start_time: Instant,
    base_timeout: Duration,
    timeout_multiplier: f64,
    consecutive_timeouts: u32,
    event_sender: mpsc::Sender<PacemakerEvent>,
    event_receiver: Option<mpsc::Receiver<PacemakerEvent>>,
}

impl Pacemaker {
    pub fn new(base_timeout: Duration, timeout_multiplier: f64) -> Self {
        let (event_sender, event_receiver) = mpsc::channel(100);
        
        Self {
            current_round: 0,
            round_start_time: Instant::now(),
            base_timeout,
            timeout_multiplier,
            consecutive_timeouts: 0,
            event_sender,
            event_receiver: Some(event_receiver),
        }
    }

    /// Get the event receiver (can only be called once)
    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<PacemakerEvent>> {
        self.event_receiver.take()
    }

    /// Get the event sender
    pub fn event_sender(&self) -> mpsc::Sender<PacemakerEvent> {
        self.event_sender.clone()
    }

    /// Start a new round with timeout
    pub async fn start_round(&mut self, round: u64) -> Result<(), HotStuffError> {
        self.current_round = round;
        self.round_start_time = Instant::now();
        
        // Start timeout for this round
        let timeout_duration = self.calculate_timeout();
        let event_sender = self.event_sender.clone();
        
        tokio::spawn(async move {
            sleep(timeout_duration).await;
            let _ = event_sender.send(PacemakerEvent::LocalTimeout(round)).await;
        });

        // Emit new round event
        self.event_sender.send(PacemakerEvent::NewRound(round)).await
            .map_err(|_| HotStuffError::Timer("Failed to send new round event".to_string()))?;

        Ok(())
    }

    /// Advance to the next round (typically called when receiving a QC)
    pub async fn advance_round(&mut self, qc: Option<&QuorumCert>) -> Result<(), HotStuffError> {
        let next_round = self.current_round + 1;
        
        // Reset consecutive timeouts if we made progress
        if qc.is_some() {
            self.consecutive_timeouts = 0;
        }
        
        self.start_round(next_round).await
    }

    /// Handle a timeout event
    pub async fn handle_timeout(&mut self, round: u64) -> Result<(), HotStuffError> {
        if round == self.current_round {
            self.consecutive_timeouts += 1;
            
            // Advance to next round due to timeout
            let next_round = self.current_round + 1;
            self.start_round(next_round).await?;
            
            // Emit new view event for view change
            self.event_sender.send(PacemakerEvent::NewView(next_round)).await
                .map_err(|_| HotStuffError::Timer("Failed to send new view event".to_string()))?;
        }
        
        Ok(())
    }

    /// Calculate timeout duration based on consecutive timeouts (exponential backoff)
    fn calculate_timeout(&self) -> Duration {
        let multiplier = self.timeout_multiplier.powi(self.consecutive_timeouts as i32);
        Duration::from_millis((self.base_timeout.as_millis() as f64 * multiplier) as u64)
    }

    /// Get current round
    pub fn current_round(&self) -> u64 {
        self.current_round
    }

    /// Get round duration so far
    pub fn round_duration(&self) -> Duration {
        self.round_start_time.elapsed()
    }

    /// Check if we should trigger a timeout for the current round
    pub fn should_timeout(&self) -> bool {
        self.round_duration() >= self.calculate_timeout()
    }

    /// Update timeout parameters
    pub fn update_timeout_params(&mut self, base_timeout: Duration, multiplier: f64) {
        self.base_timeout = base_timeout;
        self.timeout_multiplier = multiplier;
    }
}

/// A simple leader election mechanism
pub struct LeaderElection {
    nodes: Vec<u64>,
    round_robin_offset: u64,
}

impl LeaderElection {
    pub fn new(nodes: Vec<u64>) -> Self {
        Self {
            nodes,
            round_robin_offset: 0,
        }
    }

    /// Get the leader for a given round
    pub fn get_leader(&self, round: u64) -> Option<u64> {
        if self.nodes.is_empty() {
            return None;
        }
        
        let index = ((round + self.round_robin_offset) % self.nodes.len() as u64) as usize;
        self.nodes.get(index).copied()
    }

    /// Update the node list
    pub fn update_nodes(&mut self, nodes: Vec<u64>) {
        self.nodes = nodes;
    }

    /// Set round robin offset (useful for view changes)
    pub fn set_offset(&mut self, offset: u64) {
        self.round_robin_offset = offset;
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[u64] {
        &self.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_pacemaker_timeout() {
        let mut pacemaker = Pacemaker::new(Duration::from_millis(100), 1.5);
        let mut receiver = pacemaker.take_event_receiver().unwrap();
        
        // Start a round
        pacemaker.start_round(1).await.unwrap();
        
        // Should receive new round event immediately
        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PacemakerEvent::NewRound(1)));
        
        // Should receive timeout after 100ms
        let event = receiver.recv().await.unwrap();
        assert!(matches!(event, PacemakerEvent::LocalTimeout(1)));
    }

    #[test]
    fn test_leader_election() {
        let nodes = vec![0, 1, 2, 3];
        let leader_election = LeaderElection::new(nodes);
        
        assert_eq!(leader_election.get_leader(0), Some(0));
        assert_eq!(leader_election.get_leader(1), Some(1));
        assert_eq!(leader_election.get_leader(4), Some(0)); // Wraps around
    }
}
