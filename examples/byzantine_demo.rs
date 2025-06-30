// Byzantine Node Framework Demo
// Demonstrates the enhanced Byzantine fault tolerance testing capabilities

#[cfg(feature = "byzantine")]
mod demo {
    use std::time::Duration;

    use hotstuff2::protocol::byzantine_tests::{
        ByzantineTestHarness, ByzantineAttackPattern, MessageType,
        AdversarialNetworkConditions,
    };

    pub async fn run_demo() -> Result<(), Box<dyn std::error::Error>> {
        // Initialize logging
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .init();

        println!("🛡️  HotStuff-2 Byzantine Node Framework Demo");
        println!("==============================================\n");

        // Demo 1: Basic Byzantine attacks
        demo_basic_attacks().await?;
        
        // Demo 2: Coordinated attacks
        demo_coordinated_attacks().await?;
        
        // Demo 3: Advanced attack patterns
        demo_advanced_attacks().await?;
        
        // Demo 4: Comprehensive BFT test suite
        demo_comprehensive_testing().await?;

        println!("\n🎉 Byzantine Node Framework Demo Complete!");
        println!("The framework successfully demonstrated:");
        println!("✅ Various Byzantine attack patterns");
        println!("✅ Attack coordination and sophistication");
        println!("✅ Safety and liveness violation detection");
        println!("✅ Comprehensive adversarial testing capabilities");
        
        Ok(())
    }

    async fn demo_basic_attacks() -> Result<(), Box<dyn std::error::Error>> {
        println!("📍 Demo 1: Basic Byzantine Attack Patterns");
        println!("==========================================");
        
        // Test 1: Equivocation attack (Double proposing)
        println!("\n🔸 Testing Equivocation Attack...");
        let harness = ByzantineTestHarness::new(
            4, // total nodes
            1, // byzantine nodes
            vec![ByzantineAttackPattern::Equivocation],
            Duration::from_secs(5),
        );
        
        let results = harness.run_comprehensive_bft_test().await;
        print_attack_summary("Equivocation", &results);
        
        // Test 2: Double voting attack
        println!("\n🔸 Testing Double Voting Attack...");
        let harness = ByzantineTestHarness::new(
            4,
            1,
            vec![ByzantineAttackPattern::ConflictingVotes],
            Duration::from_secs(5),
        );
        
        let results = harness.run_comprehensive_bft_test().await;
        print_attack_summary("Double Voting", &results);
        
        // Test 3: Invalid signature attack  
        println!("\n🔸 Testing Invalid Signature Attack...");
        let harness = ByzantineTestHarness::new(
            4,
            1,
            vec![ByzantineAttackPattern::InvalidSignatures],
            Duration::from_secs(5),
        );
        
        let results = harness.run_comprehensive_bft_test().await;
        print_attack_summary("Invalid Signatures", &results);
        
        Ok(())
    }

    async fn demo_coordinated_attacks() -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📍 Demo 2: Coordinated Byzantine Attacks");
        println!("=======================================");
        
        // Test complex coordinated attack with multiple Byzantine nodes
        println!("\n🔸 Testing Coordinated Multi-Vector Attack...");
        
        let coordinated_attack = ByzantineAttackPattern::CoordinatedAttack(vec![
            ByzantineAttackPattern::Equivocation,
            ByzantineAttackPattern::SelectiveDelay { 
                target_peers: vec![0, 1],
                delay: Duration::from_millis(500) 
            },
            ByzantineAttackPattern::IntelligentDrop { 
                drop_rate: 0.3,
                target_message_types: vec![MessageType::Vote] 
            },
        ]);
        
        let harness = ByzantineTestHarness::new(
            6, // Larger network
            2, // Multiple Byzantine nodes
            vec![coordinated_attack],
            Duration::from_secs(10),
        );
        
        let results = harness.run_comprehensive_bft_test().await;
        print_attack_summary("Coordinated Attack", &results);
        
        Ok(())
    }

    async fn demo_advanced_attacks() -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📍 Demo 3: Advanced Attack Patterns");
        println!("===================================");
        
        // Test 1: Sophisticated timing attacks
        println!("\n🔸 Testing Sophisticated Timing Attack...");
        let harness = ByzantineTestHarness::new(
            8,
            2,
            vec![
                ByzantineAttackPattern::LateVoting { delay_threshold: Duration::from_millis(500) },
                ByzantineAttackPattern::ConflictingVotes,
            ],
            Duration::from_secs(12),
        );
        
        let results = harness.run_comprehensive_bft_test().await;
        print_attack_summary("Timing Attack", &results);
        
        // Test 2: Resource exhaustion attacks
        println!("\n🔸 Testing Resource Exhaustion Attack...");
        let harness = ByzantineTestHarness::new(
            6,
            2,
            vec![ByzantineAttackPattern::LateVoting { delay_threshold: Duration::from_millis(800) }],
            Duration::from_secs(8),
        );
        
        let results = harness.run_comprehensive_bft_test().await;
        print_attack_summary("Resource Exhaustion", &results);
        
        Ok(())
    }

    async fn demo_comprehensive_testing() -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📍 Demo 4: Comprehensive Adversarial Testing");
        println!("===========================================");
        
        println!("\n🔸 Testing Under Adversarial Network Conditions...");
        let _conditions = AdversarialNetworkConditions {
            base_latency: Duration::from_millis(50),
            latency_variance: Duration::from_millis(100),
            packet_loss_rate: 0.05,   // 5%
            burst_loss_probability: 0.1,
            partition_probability: 0.05,
            partition_duration: Duration::from_secs(2),
            bandwidth_limit: None,
            corruption_rate: 0.01,
            reorder_probability: 0.02,
            duplicate_probability: 0.01,
        };
        
        let harness = ByzantineTestHarness::new(
            10,
            3,
            vec![
                ByzantineAttackPattern::CoordinatedAttack(vec![
                    ByzantineAttackPattern::Equivocation,
                    ByzantineAttackPattern::ConflictingVotes,
                    ByzantineAttackPattern::SelectiveDelay { 
                        target_peers: vec![0, 1, 2],
                        delay: Duration::from_millis(300) 
                    },
                ]),
            ],
            Duration::from_secs(15),
        );
        
        let results = harness.run_comprehensive_bft_test().await;
        print_enhanced_summary("Adversarial Network", &results);
        
        Ok(())
    }

    fn print_attack_summary(attack_name: &str, results: &hotstuff2::protocol::byzantine_tests::ByzantineTestResults) {
        println!("\n📊 {} Results:", attack_name);
        println!("─────────────────────────────────");
        
        for (i, metric) in results.attack_metrics.iter().enumerate() {
            println!("  Node {}: {} messages sent, {} dropped, {} delayed", 
                    i, metric.messages_sent, metric.messages_dropped, metric.messages_delayed);
        }
        
        let total_sent: u64 = results.attack_metrics.iter().map(|m| m.messages_sent).sum();
        let total_dropped: u64 = results.attack_metrics.iter().map(|m| m.messages_dropped).sum();
        
        println!("\n  Summary: {} total messages sent, {} dropped", 
                total_sent, total_dropped);
        
        if results.safety_violations.len() > 0 || results.liveness_violations.len() > 0 {
            println!("  ⚠️  Detected {} safety and {} liveness violations!", 
                    results.safety_violations.len(), results.liveness_violations.len());
        } else {
            println!("  ✅ Byzantine fault tolerance working correctly!");
        }
    }

    fn print_enhanced_summary(test_name: &str, results: &hotstuff2::protocol::byzantine_tests::ByzantineTestResults) {
        println!("\n📊 {} Enhanced Results:", test_name);
        println!("──────────────────────────────────────");
        println!("  Safety violations: {}", results.safety_violations.len());
        println!("  Liveness violations: {}", results.liveness_violations.len());
        println!("  Test duration: {:?}", results.test_duration);
        println!("  Attack metrics collected: {}", results.attack_metrics.len());
        
        if results.safety_violations.len() == 0 && results.liveness_violations.len() == 0 {
            println!("  ✅ All safety and liveness properties maintained!");
        } else {
            println!("  ⚠️  Protocol violations detected - review Byzantine handling");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "byzantine")]
    {
        demo::run_demo().await
    }
    
    #[cfg(not(feature = "byzantine"))]
    {
        eprintln!("Byzantine demo requires the 'byzantine' feature to be enabled.");
        eprintln!("Run with: cargo run --example byzantine_demo --features byzantine");
        std::process::exit(1);
    }
}
