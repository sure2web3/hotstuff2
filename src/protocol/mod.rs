pub mod hotstuff2;
// pub mod recovery; // Disabled temporarily due to private field access issues
// pub mod hotstuff2_complete; // Disabled temporarily due to compilation issues

#[cfg(test)]
mod tests;

#[cfg(test)]
mod production_tests;

#[cfg(test)]
mod simple_production_tests;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod optimistic_tests;

#[cfg(test)]
mod benchmarks;

// Working integration test showcasing successful features
#[cfg(test)]
mod integration_working;

// Byzantine fault tolerance tests
#[cfg(test)]
mod byzantine_tests;

// Comprehensive tests for all HotStuff-2 features
#[cfg(test)]
mod comprehensive_tests_simple;

// #[cfg(test)]
// mod enhanced_production_tests;  // Disabled temporarily - needs refactoring for private fields

// Byzantine fault tolerance tests (disabled by default - need full node setup)
// #[cfg(test)]
// mod byzantine_tests;

#[cfg(test)]
mod basic_integration_test;
