// Byzantine Node Framework Demo
// Demonstrates the enhanced Byzantine fault tolerance testing capabilities

use std::time::Duration;
use log::{info, warn};
use tokio::time::sleep;

use hotstuff2::protocol::byzantine_tests::{
    ByzantineTestHarness, ByzantineAttackPattern, MessageType,
    EnhancedByzantineNode, AttackMetrics, AdversarialNetworkConditions,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    println!("-------------------------------------------");

    // Test equivocation attack
    println!("🔍 Testing Equivocation Attack...");
    let harness = ByzantineTestHarness::new(
        4, // 4 honest nodes
        1, // 1 Byzantine node
        vec![ByzantineAttackPattern::Equivocation],
        Duration::from_secs(5),
    );
    
    let results = harness.run_comprehensive_bft_test().await;
    print_attack_summary("Equivocation", &results.attack_metrics);

    // Test conflicting votes attack
    println!("🔍 Testing Conflicting Votes Attack...");
    let harness = ByzantineTestHarness::new(
        4, // 4 honest nodes
        1, // 1 Byzantine node
        vec![ByzantineAttackPattern::ConflictingVotes],
        Duration::from_secs(5),
    );
    
    let results = harness.run_comprehensive_bft_test().await;
    print_attack_summary("Conflicting Votes", &results.attack_metrics);

    // Test invalid signatures attack
    println!("🔍 Testing Invalid Signatures Attack...");
    let harness = ByzantineTestHarness::new(
        4, // 4 honest nodes
        1, // 1 Byzantine node
        vec![ByzantineAttackPattern::InvalidSignatures],
        Duration::from_secs(5),
    );
    
    let results = harness.run_comprehensive_bft_test().await;
    print_attack_summary("Invalid Signatures", &results.attack_metrics);

    println!("✅ Basic attacks completed.\n");
    Ok(())
}

async fn demo_coordinated_attacks() -> Result<(), Box<dyn std::error::Error>> {
    println!("📍 Demo 2: Coordinated Byzantine Attacks");
    println!("----------------------------------------");

    // Coordinated attack combining multiple strategies
    println!("🔍 Testing Coordinated Multi-Vector Attack...");
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
        6, // 6 honest nodes
        2, // 2 Byzantine nodes
        vec![coordinated_attack],
        Duration::from_secs(10),
    );

    let results = harness.run_comprehensive_bft_test().await;
    print_attack_summary("Coordinated Attack", &results.attack_metrics);
    
    if !results.liveness_violations.is_empty() {
        println!("⚠️  Liveness violations detected (expected with coordinated attacks):");
        for violation in &results.liveness_violations {
            println!("   - {:?}: {} (duration: {:?})", 
                     violation.violation_type, violation.details, violation.duration);
        }
    }

    println!("✅ Coordinated attacks completed.\n");
    Ok(())
}

async fn demo_advanced_attacks() -> Result<(), Box<dyn std::error::Error>> {
    println!("📍 Demo 3: Advanced Attack Patterns");
    println!("-----------------------------------");

    // Nothing-at-stake attack
    println!("🔍 Testing Nothing-at-Stake Attack...");
    let harness = ByzantineTestHarness::new(
        4, // 4 honest nodes
        1, // 1 Byzantine node
        vec![ByzantineAttackPattern::NothingAtStake],
        Duration::from_secs(6),
    );
    
    let results = harness.run_comprehensive_bft_test().await;
    print_attack_summary("Nothing-at-Stake", &results.attack_metrics);

    // DoS attack
    println!("🔍 Testing Denial of Service Attack...");
    let harness = ByzantineTestHarness::new(
        5, // 5 honest nodes
        1, // 1 Byzantine node
        vec![ByzantineAttackPattern::DenialOfService { message_rate: 50 }],
        Duration::from_secs(4),
    );
    
    let results = harness.run_comprehensive_bft_test().await;
    print_attack_summary("DoS Attack", &results.attack_metrics);

    // Late voting attack
    println!("🔍 Testing Late Voting Attack...");
    let harness = ByzantineTestHarness::new(
        4, // 4 honest nodes
        1, // 1 Byzantine node
        vec![ByzantineAttackPattern::LateVoting { delay_threshold: Duration::from_millis(800) }],
        Duration::from_secs(6),
    );
    
    let results = harness.run_comprehensive_bft_test().await;
    print_attack_summary("Late Voting", &results.attack_metrics);

    println!("✅ Advanced attacks completed.\n");
    Ok(())
}

async fn demo_comprehensive_testing() -> Result<(), Box<dyn std::error::Error>> {
    println!("📍 Demo 4: Comprehensive BFT Test Suite");
    println!("---------------------------------------");

    // Mixed Byzantine behaviors with multiple nodes
    println!("🔍 Running Comprehensive Multi-Node Byzantine Test...");
    let attack_patterns = vec![
        ByzantineAttackPattern::Equivocation,
        ByzantineAttackPattern::ConflictingVotes,
        ByzantineAttackPattern::ViewChangeAttack,
        ByzantineAttackPattern::CorruptedMessages,
    ];

    let harness = ByzantineTestHarness::new(
        7, // 7 honest nodes (majority)
        4, // 4 Byzantine nodes with different behaviors
        attack_patterns,
        Duration::from_secs(15),
    );

    let results = harness.run_comprehensive_bft_test().await;
    
    println!("\n📊 Comprehensive Test Results:");
    results.print_summary();
    
    // Verify system resilience
    if results.safety_violations.is_empty() {
        println!("✅ System maintained safety under Byzantine attacks!");
    } else {
        println!("❌ Safety violations detected:");
        for violation in &results.safety_violations {
            println!("   - {:?}: {}", violation.violation_type, violation.details);
        }
    }

    // Show scenario results
    println!("\n📋 Test Scenario Results:");
    for result in &results.scenario_results {
        let status = if result.success { "✅ PASSED" } else { "❌ FAILED" };
        println!("   {} {}: {} ({:?})", 
                 status, result.scenario_name, result.details, result.duration);
    }

    // Network conditions summary
    println!("\n🌐 Network Conditions:");
    let conditions = &results.network_conditions;
    println!("   - Base latency: {:?}", conditions.base_latency);
    println!("   - Packet loss rate: {:.1}%", conditions.packet_loss_rate * 100.0);
    println!("   - Corruption rate: {:.1}%", conditions.corruption_rate * 100.0);

    println!("✅ Comprehensive testing completed.\n");
    Ok(())
}

fn print_attack_summary(attack_name: &str, metrics: &[AttackMetrics]) {
    if !metrics.is_empty() {
        let total_messages = metrics.iter().map(|m| m.messages_sent).sum::<u64>();
        let total_equivocations = metrics.iter().map(|m| m.equivocations_created).sum::<u64>();
        let total_drops = metrics.iter().map(|m| m.messages_dropped).sum::<u64>();
        let total_delays = metrics.iter().map(|m| m.messages_delayed).sum::<u64>();
        let safety_attempts = metrics.iter().map(|m| m.safety_violations_attempted).sum::<u64>();
        let liveness_attempts = metrics.iter().map(|m| m.liveness_violations_attempted).sum::<u64>();

        println!("   📈 Attack Metrics for {}:", attack_name);
        println!("      - Messages sent: {}", total_messages);
        println!("      - Equivocations: {}", total_equivocations);
        println!("      - Messages dropped: {}", total_drops);
        println!("      - Messages delayed: {}", total_delays);
        println!("      - Safety violation attempts: {}", safety_attempts);
        println!("      - Liveness violation attempts: {}", liveness_attempts);
    }
}
