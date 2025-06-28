use crate::types::{Block, Hash, QuorumCert};
use crate::error::HotStuffError;
use crate::consensus::ChainView;

/// Safety engine implementing HotStuff-2 safety rules
pub struct SafetyEngine {
    locked_qc: Option<QuorumCert>,
    preferred_round: u64,
    last_voted_round: u64,
    safety_violations: Vec<SafetyViolation>,
}

#[derive(Debug, Clone)]
pub struct SafetyViolation {
    pub violation_type: ViolationType,
    pub round: u64,
    pub block_hash: Hash,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ViolationType {
    DoubleVoting,
    VotingWithoutLock,
    InvalidChainExtension,
    ConflictingLock,
}

/// Result of comprehensive safety check
#[derive(Debug, Clone)]
pub struct SafetyResult {
    pub is_safe: bool,
    pub warnings: Vec<String>,
    pub can_update_lock: bool,
    pub should_commit: bool,
}

/// Safety statistics for monitoring
#[derive(Debug, Clone)]
pub struct SafetyStats {
    pub total_violations: usize,
    pub double_voting_violations: usize,
    pub lock_violations: usize,
    pub chain_violations: usize,
    pub last_voted_round: u64,
    pub has_lock: bool,
}

impl SafetyEngine {
    pub fn new() -> Self {
        Self {
            locked_qc: None,
            preferred_round: 0,
            last_voted_round: 0,
            safety_violations: Vec::new(),
        }
    }

    /// Core safety rule: determine if it's safe to vote for a block
    pub fn safe_to_vote(
        &mut self,
        block: &Block,
        round: u64,
        chain_view: &ChainView,
    ) -> Result<bool, HotStuffError> {
        // Rule 1: Never vote twice in the same round
        if round <= self.last_voted_round {
            self.record_violation(SafetyViolation {
                violation_type: ViolationType::DoubleVoting,
                round,
                block_hash: block.hash(),
                description: format!("Attempted to vote in round {} when last vote was in round {}", 
                                   round, self.last_voted_round),
            });
            return Ok(false);
        }

        // Rule 2: HotStuff-2 safety rule - can only vote if:
        // a) Block extends locked QC, OR
        // b) We have a higher QC than our locked QC
        if let Some(locked_qc) = &self.locked_qc {
            let extends_locked = chain_view.extends_qc(block, locked_qc);
            
            if !extends_locked {
                // Check if we have a higher QC
                let has_higher_qc = chain_view.qc_chain.iter()
                    .any(|qc| qc.height > locked_qc.height);
                
                if !has_higher_qc {
                    self.record_violation(SafetyViolation {
                        violation_type: ViolationType::VotingWithoutLock,
                        round,
                        block_hash: block.hash(),
                        description: format!("Block {} does not extend locked QC {}", 
                                           block.hash(), locked_qc.block_hash),
                    });
                    return Ok(false);
                }
            }
        }

        // Rule 3: Block should be well-formed and extend a known block
        if !self.validate_block_structure(block, chain_view)? {
            self.record_violation(SafetyViolation {
                violation_type: ViolationType::InvalidChainExtension,
                round,
                block_hash: block.hash(),
                description: "Block does not properly extend the chain".to_string(),
            });
            return Ok(false);
        }

        Ok(true)
    }

    /// Vote for a block (updates last voted round)
    pub fn vote(&mut self, block: &Block, round: u64) -> Result<(), HotStuffError> {
        if !self.safe_to_vote(block, round, &ChainView::new())? {
            return Err(HotStuffError::Consensus("Unsafe to vote".to_string()));
        }
        
        self.last_voted_round = round;
        self.preferred_round = self.preferred_round.max(round);
        
        Ok(())
    }

    /// Update locked QC (typically in pre-commit phase)
    pub fn update_locked_qc(&mut self, qc: QuorumCert) -> Result<(), HotStuffError> {
        // Verify that this QC is higher than our current locked QC
        if let Some(current_locked) = &self.locked_qc {
            if qc.height <= current_locked.height {
                self.record_violation(SafetyViolation {
                    violation_type: ViolationType::ConflictingLock,
                    round: 0, // Round not applicable here
                    block_hash: qc.block_hash,
                    description: format!("Attempted to lock on QC {} when already locked on {}", 
                                       qc.height, current_locked.height),
                });
                return Err(HotStuffError::Consensus("Cannot lock on lower QC".to_string()));
            }
        }

        self.locked_qc = Some(qc);
        Ok(())
    }

    /// Check if we can commit according to three-chain rule
    pub fn can_commit(&self, qc_chain: &[QuorumCert]) -> Option<Hash> {
        if qc_chain.len() < 3 {
            return None;
        }

        // Check last three QCs form a valid chain
        let len = qc_chain.len();
        let qc1 = &qc_chain[len - 3];
        let qc2 = &qc_chain[len - 2];
        let qc3 = &qc_chain[len - 1];

        // Three-chain rule: consecutive heights and proper nesting
        if qc2.height == qc1.height + 1 && qc3.height == qc2.height + 1 {
            Some(qc1.block_hash)
        } else {
            None
        }
    }

    /// Validate block structure and chain extension
    fn validate_block_structure(&self, block: &Block, chain_view: &ChainView) -> Result<bool, HotStuffError> {
        // Check if parent exists in our chain view
        if block.parent_hash != Hash::zero() {
            if chain_view.get_block(&block.parent_hash).is_none() {
                return Ok(false);
            }
        }

        // Check height progression
        if let Some(parent) = chain_view.get_block(&block.parent_hash) {
            if block.height != parent.height + 1 {
                return Ok(false);
            }
        }

        // Additional structure validation could go here
        Ok(true)
    }

    /// Record a safety violation for debugging/monitoring
    fn record_violation(&mut self, violation: SafetyViolation) {
        self.safety_violations.push(violation);
        
        // Keep only recent violations
        if self.safety_violations.len() > 100 {
            self.safety_violations.remove(0);
        }
    }

    /// Enhanced safety check with comprehensive validation
    pub fn comprehensive_safety_check(
        &mut self,
        block: &Block,
        round: u64,
        proposed_qc: Option<&QuorumCert>,
        chain_view: &ChainView,
    ) -> Result<SafetyResult, HotStuffError> {
        let mut warnings = Vec::new();
        let mut is_safe = true;

        // Basic safety checks
        if round <= self.last_voted_round {
            self.record_violation(SafetyViolation {
                violation_type: ViolationType::DoubleVoting,
                round,
                block_hash: block.hash(),
                description: format!("Attempted to vote in round {} but already voted in round {}", 
                                   round, self.last_voted_round),
            });
            is_safe = false;
        }

        // Chain extension validation
        if !self.validates_chain_extension(block, chain_view)? {
            warnings.push("Block does not properly extend the chain".to_string());
        }

        // QC validation if provided
        if let Some(qc) = proposed_qc {
            if !self.validates_qc_safety(qc, block)? {
                self.record_violation(SafetyViolation {
                    violation_type: ViolationType::ConflictingLock,
                    round,
                    block_hash: block.hash(),
                    description: "QC conflicts with safety rules".to_string(),
                });
                is_safe = false;
            }
        }

        // Lock validation
        if let Some(locked_qc) = &self.locked_qc {
            if !self.respects_locked_qc(block, locked_qc, chain_view)? {
                self.record_violation(SafetyViolation {
                    violation_type: ViolationType::VotingWithoutLock,
                    round,
                    block_hash: block.hash(),
                    description: "Block does not respect locked QC".to_string(),
                });
                is_safe = false;
            }
        }

        Ok(SafetyResult {
            is_safe,
            warnings,
            can_update_lock: self.can_update_lock(block, round),
            should_commit: self.should_commit_with_qc(proposed_qc),
        })
    }

    /// Validate chain extension
    fn validates_chain_extension(&self, block: &Block, chain_view: &ChainView) -> Result<bool, HotStuffError> {
        // Check if block extends the current chain properly
        if block.height == 0 {
            return Ok(true); // Genesis block
        }

        // Verify parent exists and height is correct
        if let Some(parent) = chain_view.get_block(&block.parent_hash) {
            Ok(block.height == parent.height + 1)
        } else {
            Ok(false) // Parent not found
        }
    }

    /// Validate QC safety
    fn validates_qc_safety(&self, qc: &QuorumCert, block: &Block) -> Result<bool, HotStuffError> {
        // Ensure QC height is reasonable
        if qc.height > block.height {
            return Ok(false);
        }

        // Check if QC conflicts with our locked QC
        if let Some(locked_qc) = &self.locked_qc {
            if qc.height <= locked_qc.height && qc.block_hash != locked_qc.block_hash {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Check if block respects locked QC
    fn respects_locked_qc(&self, block: &Block, locked_qc: &QuorumCert, chain_view: &ChainView) -> Result<bool, HotStuffError> {
        // If no lock, any block is fine
        if self.locked_qc.is_none() {
            return Ok(true);
        }

        // Block must extend the locked QC or be at a higher round
        if block.height > locked_qc.height {
            // Check if block extends locked block
            self.extends_block_transitively(block, &locked_qc.block_hash, chain_view)
        } else {
            Ok(false) // Cannot vote for block at same or lower height than lock
        }
    }

    /// Check if block extends another block transitively
    fn extends_block_transitively(&self, block: &Block, target_hash: &Hash, chain_view: &ChainView) -> Result<bool, HotStuffError> {
        let mut current_hash = block.parent_hash;
        let mut depth = 0;
        const MAX_DEPTH: usize = 1000;

        while depth < MAX_DEPTH {
            if current_hash == *target_hash {
                return Ok(true);
            }

            if let Some(parent) = chain_view.get_block(&current_hash) {
                current_hash = parent.parent_hash;
                depth += 1;
            } else {
                break;
            }
        }

        Ok(false)
    }

    /// Check if we can update our lock
    fn can_update_lock(&self, _block: &Block, round: u64) -> bool {
        match &self.locked_qc {
            None => true, // No current lock
            Some(locked_qc) => round > locked_qc.height, // Higher round than current lock
        }
    }

    /// Check if we should commit with this QC
    fn should_commit_with_qc(&self, qc: Option<&QuorumCert>) -> bool {
        if let Some(qc) = qc {
            if let Some(locked_qc) = &self.locked_qc {
                // HotStuff-2 commit rule: commit if consecutive QCs
                return qc.height == locked_qc.height + 1;
            }
        }
        false
    }

    /// Get safety statistics
    pub fn get_safety_stats(&self) -> SafetyStats {
        SafetyStats {
            total_violations: self.safety_violations.len(),
            double_voting_violations: self.safety_violations.iter()
                .filter(|v| matches!(v.violation_type, ViolationType::DoubleVoting))
                .count(),
            lock_violations: self.safety_violations.iter()
                .filter(|v| matches!(v.violation_type, ViolationType::VotingWithoutLock))
                .count(),
            chain_violations: self.safety_violations.iter()
                .filter(|v| matches!(v.violation_type, ViolationType::InvalidChainExtension))
                .count(),
            last_voted_round: self.last_voted_round,
            has_lock: self.locked_qc.is_some(),
        }
    }

    /// Reset safety state (for testing or recovery)
    pub fn reset(&mut self) {
        self.locked_qc = None;
        self.preferred_round = 0;
        self.last_voted_round = 0;
        self.safety_violations.clear();
    }

    /// Get locked QC
    pub fn locked_qc(&self) -> Option<&QuorumCert> {
        self.locked_qc.as_ref()
    }

    /// Get preferred round
    pub fn preferred_round(&self) -> u64 {
        self.preferred_round
    }

    /// Get last voted round
    pub fn last_voted_round(&self) -> u64 {
        self.last_voted_round
    }

    /// Get recent safety violations
    pub fn recent_violations(&self) -> &[SafetyViolation] {
        &self.safety_violations
    }

    /// Reset safety state (used for view changes)
    pub fn reset_for_view_change(&mut self, new_round: u64) {
        self.preferred_round = new_round;
        // Note: we keep locked_qc for safety
    }
}

/// Liveness engine for ensuring the protocol makes progress
pub struct LivenessEngine {
    consecutive_timeouts: u32,
    max_consecutive_timeouts: u32,
    view_change_threshold: u32,
}

impl LivenessEngine {
    pub fn new(max_consecutive_timeouts: u32, view_change_threshold: u32) -> Self {
        Self {
            consecutive_timeouts: 0,
            max_consecutive_timeouts,
            view_change_threshold,
        }
    }

    /// Record a timeout
    pub fn record_timeout(&mut self) {
        self.consecutive_timeouts += 1;
    }

    /// Record progress (reset timeout counter)
    pub fn record_progress(&mut self) {
        self.consecutive_timeouts = 0;
    }

    /// Check if we should trigger a view change
    pub fn should_view_change(&self) -> bool {
        self.consecutive_timeouts >= self.view_change_threshold
    }

    /// Check if we're in a liveness failure state
    pub fn is_liveness_failure(&self) -> bool {
        self.consecutive_timeouts >= self.max_consecutive_timeouts
    }

    /// Get current timeout count
    pub fn consecutive_timeouts(&self) -> u32 {
        self.consecutive_timeouts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Transaction, Timestamp};

    #[test]
    fn test_safety_double_voting() {
        let mut safety = SafetyEngine::new();
        let chain_view = ChainView::new();
        
        let block = Block::new(Hash::zero(), vec![], 1, 0);
        
        // First vote should be safe
        assert!(safety.safe_to_vote(&block, 1, &chain_view).unwrap());
        safety.vote(&block, 1).unwrap();
        
        // Second vote in same round should be unsafe
        assert!(!safety.safe_to_vote(&block, 1, &chain_view).unwrap());
        assert_eq!(safety.recent_violations().len(), 1);
    }

    #[test]
    fn test_three_chain_commit() {
        let safety = SafetyEngine::new();
        
        let qc1 = QuorumCert::new(Hash::zero(), 1, vec![]);
        let qc2 = QuorumCert::new(Hash::zero(), 2, vec![]);
        let qc3 = QuorumCert::new(Hash::zero(), 3, vec![]);
        
        let qc_chain = vec![qc1.clone(), qc2, qc3];
        
        // Should be able to commit qc1's block
        assert_eq!(safety.can_commit(&qc_chain), Some(qc1.block_hash));
    }
}
