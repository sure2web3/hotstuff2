//! Minimal Byzantine test to debug hanging issue

use std::time::Duration;
use tokio::time::sleep;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::byzantine_tests::{ByzantineTestHarness, ByzantineAttackPattern};

    #[tokio::test]
    async fn minimal_dos_test() {
        let _ = env_logger::builder().is_test(true).try_init();
        
        println!("🔥 Starting minimal DoS test");
        
        let attack_pattern = ByzantineAttackPattern::DenialOfService { message_rate: 5 };
        println!("✅ Created attack pattern");
        
        let _harness = ByzantineTestHarness::new(
            3, // 3 honest nodes
            1, // 1 Byzantine node
            vec![attack_pattern],
            Duration::from_millis(100), // Very short duration
        );
        println!("✅ Created test harness");
        
        // Just sleep for a bit instead of running full test
        sleep(Duration::from_millis(50)).await;
        println!("✅ Sleep completed");
        
        println!("🎉 Minimal test completed successfully");
    }

    #[tokio::test]
    async fn super_minimal_test() {
        println!("🚀 Super minimal test starting");
        sleep(Duration::from_millis(10)).await;
        println!("✅ Super minimal test completed");
    }
}
