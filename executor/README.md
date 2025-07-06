# HotStuff-2 Transaction Executor

**Transaction execution engine** designed for deterministic and high-performance execution of transactions within HotStuff-2 consensus blocks.

## 🎯 Design Philosophy

The HotStuff-2 executor provides **deterministic transaction execution** with support for smart contracts, state transitions, and complex business logic while maintaining consensus safety.

### Core Principles

1. **Deterministic Execution**: Guaranteed identical results across all validators
2. **High Performance**: Optimized execution for maximum transaction throughput
3. **Gas Metering**: Resource usage tracking and limits
4. **Virtual Machine Support**: Multiple execution environments
5. **State Isolation**: Safe execution without interference between transactions

## 🏗️ Architecture Overview

### Executor Components

```
┌─────────────────────────────────────────┐
│         HotStuff-2 Consensus             │
├─────────────────────────────────────────┤
│       Block Execution Integration       │  ← Consensus Events
├─────────────────────────────────────────┤
│ Transaction │   VM      │   Gas      │  ← Execution Engine
│  Executor   │  Engine   │ Tracker    │
├─────────────────────────────────────────┤
│   State     │  Memory   │ Exception  │  ← Supporting Systems
│  Manager    │ Manager   │ Handler    │
└─────────────────────────────────────────┘
```

## 🔧 Core Executor Interface

### `TransactionExecutor` Trait

**Purpose**: Unified interface for deterministic transaction execution.

```rust
#[async_trait]
pub trait TransactionExecutor: Send + Sync {
    // Transaction Execution
    async fn execute_transaction(&mut self, tx: &Transaction, context: &ExecutionContext) -> ExecutionResult;
    async fn execute_block(&mut self, block: &Block, context: &BlockExecutionContext) -> BlockExecutionResult;
    async fn validate_transaction(&self, tx: &Transaction, context: &ValidationContext) -> ValidationResult;
    
    // State Management
    async fn apply_state_changes(&mut self, changes: StateChanges) -> ExecutorResult<()>;
    async fn revert_state_changes(&mut self, changes: &StateChanges) -> ExecutorResult<()>;
    async fn get_state_root(&self) -> ExecutorResult<Hash>;
    
    // Gas and Resource Management
    async fn estimate_gas(&self, tx: &Transaction, context: &EstimationContext) -> EstimationResult;
    async fn track_resource_usage(&mut self, resources: ResourceUsage) -> ExecutorResult<()>;
    async fn check_resource_limits(&self, usage: &ResourceUsage, limits: &ResourceLimits) -> bool;
    
    // Virtual Machine Integration
    async fn execute_contract(&mut self, contract_call: &ContractCall, context: &VMContext) -> VMResult;
    async fn deploy_contract(&mut self, deployment: &ContractDeployment, context: &VMContext) -> VMResult;
}
```

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module contains interface definitions and architectural design for the HotStuff-2 transaction executor.

**Current State**: 
- ✅ Executor interface design
- ✅ Execution architecture planning
- ⏳ Implementation pending

## 🔗 Integration Points

- **consensus/**: Block execution integration
- **state/**: State management integration
- **types/**: Transaction and block type definitions
- **crypto/**: Cryptographic operations
