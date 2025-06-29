use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;

use crate::config::HotStuffConfig;
use crate::consensus::state_machine::KVStateMachine;
use crate::crypto::KeyPair;
use crate::network::NetworkClient;
use crate::protocol::hotstuff2::HotStuff2;
use crate::storage::block_store::MemoryBlockStore;
use crate::timer::TimeoutManager;
use crate::types::Transaction;

#[tokio::test]
async fn test_basic_node_creation() {
    // Test that we can create a basic HotStuff2 node with all the fixed components
    let mut rng = rand::rng();
    let key_pair = KeyPair::generate(&mut rng);
    let block_store = Arc::new(MemoryBlockStore::new());
    let peers = HashMap::new();
    let network_client = Arc::new(NetworkClient::new(0, peers));
    let timeout_manager = TimeoutManager::new(Duration::from_secs(5), 1.5);
    let config = HotStuffConfig::default_for_testing();
    let state_machine = Arc::new(tokio::sync::Mutex::new(KVStateMachine::new()));
    
    // This should not panic
    let node = HotStuff2::new(
        0,
        key_pair,
        network_client,
        block_store,
        timeout_manager,
        4,
        config,
        state_machine,
    );
    
    // Test that we can access the node ID
    assert_eq!(node.get_node_id(), 0);
    
    // Test that we can get performance statistics
    let stats = node.get_performance_statistics().await.unwrap();
    assert_eq!(stats.current_height, 0);
}

#[tokio::test]
async fn test_transaction_submission() {
    let mut rng = rand::rng();
    let key_pair = KeyPair::generate(&mut rng);
    let block_store = Arc::new(MemoryBlockStore::new());
    let peers = HashMap::new();
    let network_client = Arc::new(NetworkClient::new(0, peers));
    let timeout_manager = TimeoutManager::new(Duration::from_secs(5), 1.5);
    let config = HotStuffConfig::default_for_testing();
    let state_machine = Arc::new(tokio::sync::Mutex::new(KVStateMachine::new()));
    
    let node = HotStuff2::new(
        0,
        key_pair,
        network_client,
        block_store,
        timeout_manager,
        4,
        config,
        state_machine,
    );
    
    // Test transaction submission
    let transaction = Transaction::new(
        "test_tx_1".to_string(),
        b"test transaction data".to_vec(),
    );
    
    // This should work without errors
    let result = node.submit_transaction(transaction).await;
    assert!(result.is_ok());
}
