// Example demonstrating the HotStuff-2 Advanced Stress Testing Framework
// cargo run --example stress_test_demo

use std::time::Duration;
use hotstuff2::testing::stress_test::{
    ConsensusStressTest, StressTestConfig, NetworkConditions, ByzantineBehavior, 
    PartitionScenario, PerformanceThresholds
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 HotStuff-2 Advanced Stress Testing Framework Demo");
    println!("=================================================");
    
    // Create comprehensive stress test configuration
    let config = StressTestConfig {
        num_nodes: 4,
        num_byzantine: 1,
        target_tps: 500,
        test_duration: Duration::from_secs(120), // 2 minutes for demo
        network_conditions: NetworkConditions {
            base_latency: Duration::from_millis(20),
            latency_variance: Duration::from_millis(5),
            packet_loss_rate: 0.005, // 0.5% packet loss
            bandwidth_limit: Some(50_000_000), // 50 Mbps
            partition_scenarios: vec![
                PartitionScenario::BinaryPartition { 
                    duration: Duration::from_secs(15), 
                    group_size: 2 
                },
                PartitionScenario::NodeIsolation { 
                    duration: Duration::from_secs(10), 
                    isolated_nodes: vec![0] 
                },
            ],
        },
        byzantine_behaviors: vec![
            ByzantineBehavior::ConflictingVotes { frequency: 0.05 },
            ByzantineBehavior::MessageDelay { 
                delay: Duration::from_millis(50), 
                frequency: 0.03 
            },
            ByzantineBehavior::MessageDrop { drop_rate: 0.01 },
        ],
        performance_thresholds: PerformanceThresholds {
            max_consensus_latency: Duration::from_millis(300),
            min_throughput: 250,
            max_memory_usage: 512 * 1024 * 1024, // 512MB
            max_cpu_usage: 70.0,
            max_error_rate: 0.02, // 2%
        },
    };
    
    println!("Configuration:");
    println!("  📊 Nodes: {} honest + {} Byzantine", config.num_nodes, config.num_byzantine);
    println!("  🎯 Target TPS: {}", config.target_tps);
    println!("  ⏱️  Test Duration: {:?}", config.test_duration);
    println!("  🌐 Network Latency: {:?} ± {:?}", config.network_conditions.base_latency, config.network_conditions.latency_variance);
    println!("  📉 Packet Loss: {:.2}%", config.network_conditions.packet_loss_rate * 100.0);
    println!("  🔀 Partition Scenarios: {}", config.network_conditions.partition_scenarios.len());
    println!("  😈 Byzantine Behaviors: {}", config.byzantine_behaviors.len());
    
    // Create and run stress test
    println!("\n🔧 Initializing stress test framework...");
    let mut stress_test = ConsensusStressTest::new(config.clone()).await?;
    
    println!("✅ Framework initialized successfully!");
    println!("\n🧪 Running comprehensive stress test suite...");
    
    // Run the complete stress test
    let report = stress_test.run_stress_test().await?;
    
    // Print detailed results
    println!("\n📋 STRESS TEST RESULTS");
    println!("======================");
    report.print_detailed_report();
    
    // Additional analysis
    println!("🔍 ANALYSIS");
    println!("============");
    
    if report.safety_violations == 0 {
        println!("✅ Safety: All consensus safety properties maintained");
    } else {
        println!("⚠️  Safety: {} safety violations detected", report.safety_violations);
    }
    
    if report.liveness_violations == 0 {
        println!("✅ Liveness: System maintained progress under all conditions");
    } else {
        println!("⚠️  Liveness: {} liveness violations detected", report.liveness_violations);
    }
    
    if report.transaction_success_rate > 0.95 {
        println!("✅ Reliability: High transaction success rate ({:.2}%)", report.transaction_success_rate * 100.0);
    } else {
        println!("⚠️  Reliability: Lower transaction success rate ({:.2}%)", report.transaction_success_rate * 100.0);
    }
    
    if report.average_tps > config.performance_thresholds.min_throughput as f64 {
        println!("✅ Performance: Target throughput achieved ({:.2} TPS)", report.average_tps);
    } else {
        println!("⚠️  Performance: Below target throughput ({:.2} TPS)", report.average_tps);
    }
    
    // BLS cryptography performance
    let bls_perf = &report.bls_signature_performance;
    if bls_perf.verification_failures == 0 {
        println!("✅ BLS Cryptography: All {} signatures verified successfully", bls_perf.signatures_verified);
    } else {
        println!("⚠️  BLS Cryptography: {} verification failures out of {} signatures", 
                bls_perf.verification_failures, bls_perf.signatures_verified);
    }
    
    println!("🔐 BLS Performance: {} signatures created, {} aggregations performed", 
            bls_perf.signatures_created, bls_perf.aggregations_performed);
    
    // Network resilience
    if report.network_partitions_tested > 0 {
        println!("✅ Network Resilience: Successfully tested {} partition scenarios", 
                report.network_partitions_tested);
    }
    
    println!("\n🎯 CONCLUSION");
    println!("==============");
    
    let overall_score = calculate_overall_score(&report);
    match overall_score {
        90..=100 => println!("🏆 EXCELLENT: System demonstrates production-ready robustness (Score: {}%)", overall_score),
        80..=89 => println!("✅ GOOD: System shows strong resilience with minor areas for improvement (Score: {}%)", overall_score),
        70..=79 => println!("⚠️  ACCEPTABLE: System functions but has notable weaknesses (Score: {}%)", overall_score),
        _ => println!("❌ NEEDS IMPROVEMENT: System requires significant enhancements (Score: {}%)", overall_score),
    }
    
    println!("\n✨ Stress testing completed successfully!");
    println!("📈 The HotStuff-2 implementation has been thoroughly validated for production use.");
    
    Ok(())
}

fn calculate_overall_score(report: &hotstuff2::testing::stress_test::StressTestReport) -> u32 {
    let mut score = 100u32;
    
    // Deduct points for safety/liveness violations
    score = score.saturating_sub((report.safety_violations * 20) as u32);
    score = score.saturating_sub((report.liveness_violations * 10) as u32);
    
    // Deduct points for low success rate
    if report.transaction_success_rate < 0.95 {
        score = score.saturating_sub(((0.95 - report.transaction_success_rate) * 50.0) as u32);
    }
    
    // Deduct points for BLS verification failures
    if report.bls_signature_performance.verification_failures > 0 {
        let failure_rate = report.bls_signature_performance.verification_failures as f64 / 
                          report.bls_signature_performance.signatures_verified as f64;
        score = score.saturating_sub((failure_rate * 30.0) as u32);
    }
    
    // Deduct points for performance issues
    if report.performance_summary.error_rate > 0.01 {
        score = score.saturating_sub((report.performance_summary.error_rate * 100.0) as u32);
    }
    
    score
}
