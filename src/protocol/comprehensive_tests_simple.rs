use crate::consensus::state_machine::StateMachine;
use crate::error::HotStuffError;
use crate::types::{Block, Hash};

/// A simple mock state machine for testing
#[derive(Debug)]
pub struct SimpleMockStateMachine {
    executed_blocks: Vec<Hash>,
    current_height: u64,
}

impl SimpleMockStateMachine {
    pub fn new() -> Self {
        Self {
            executed_blocks: Vec::new(),
            current_height: 0,
        }
    }
}

impl StateMachine for SimpleMockStateMachine {
    fn execute_block(&mut self, block: &Block) -> Result<Hash, HotStuffError> {
        // Simple execution - just record that we executed this block
        self.executed_blocks.push(block.hash());
        self.current_height = block.height;
        Ok(block.hash())
    }

    fn state_hash(&self) -> Hash {
        // Simple state hash based on executed blocks
        let mut data = Vec::new();
        for block_hash in &self.executed_blocks {
            data.extend_from_slice(block_hash.as_bytes());
        }
        Hash::from_bytes(&data)
    }

    fn height(&self) -> u64 {
        self.current_height
    }

    fn reset_to_state(&mut self, _state_hash: Hash, height: u64) -> Result<(), HotStuffError> {
        // Simple reset - just update our state
        self.current_height = height;
        // Note: In a real implementation, we'd reconstruct state from the state_hash
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mock_state_machine() {
        let mut state_machine = SimpleMockStateMachine::new();
        
        // Test initial state
        assert_eq!(state_machine.height(), 0);
        
        // Create a dummy block for testing
        let block = Block::new(
            Hash::from_bytes(b"parent_hash"),
            vec![], // empty transactions
            1,
            0,
        );
        
        // Execute the block
        let result = state_machine.execute_block(&block);
        assert!(result.is_ok());
        
        // Check that height was updated
        assert_eq!(state_machine.height(), 1);
        
        // Test state hash generation (should not be empty)
        let state_hash = state_machine.state_hash();
        assert_ne!(state_hash.as_bytes().len(), 0);
        
        // Test reset functionality
        let reset_result = state_machine.reset_to_state(state_hash, 5);
        assert!(reset_result.is_ok());
        assert_eq!(state_machine.height(), 5);
        
        println!("Mock state machine test completed successfully");
    }
    
    #[tokio::test]
    async fn test_state_machine_multiple_blocks() {
        let mut state_machine = SimpleMockStateMachine::new();
        
        // Execute multiple blocks
        for i in 1..=5 {
            let block = Block::new(
                Hash::from_bytes(&format!("parent_hash_{}", i-1).into_bytes()),
                vec![], // empty transactions
                i,
                0,
            );
            
            let result = state_machine.execute_block(&block);
            assert!(result.is_ok());
            assert_eq!(state_machine.height(), i);
        }
        
        // Check that we executed all blocks
        assert_eq!(state_machine.executed_blocks.len(), 5);
        
        println!("Multiple blocks test completed successfully");
    }
}
