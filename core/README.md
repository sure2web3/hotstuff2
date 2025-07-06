# HotStuff-2 Core Protocol Logic

This module implements the **fundamental protocol logic** and state management for the HotStuff-2 consensus algorithm. It provides the core building blocks that the consensus layer uses to implement the complete HotStuff-2 protocol.

## 🎯 Core Responsibilities

### Protocol Foundation
- **State Management**: Core protocol state and transitions
- **Safety Rules**: Fundamental safety property enforcement
- **Liveness Logic**: Progress guarantee mechanisms
- **Message Validation**: Protocol message verification

### Key Components

#### Protocol State Machine (`protocol.rs`)
- **Core State**: Maintains essential protocol state (view, height, locked blocks)
- **State Transitions**: Manages valid protocol state changes
- **Invariant Checking**: Ensures protocol correctness properties
- **Checkpoint Management**: Periodic state consistency verification

#### Safety Module (`safety.rs`)
- **Voting Rules**: Prevents conflicting votes from honest nodes
- **Lock Mechanism**: Ensures safety across view changes
- **Conflict Detection**: Identifies potential safety violations
- **Byzantine Protection**: Guards against malicious behavior

#### Liveness Module (`liveness.rs`)
- **Progress Tracking**: Monitors consensus progress
- **Timeout Management**: Handles stuck consensus scenarios
- **View Advancement**: Ensures eventual progress under synchrony
- **Recovery Mechanisms**: Restores progress after partitions

#### Pacemaker (`pacemaker.rs`)
- **View Synchronization**: Coordinates view changes across validators
- **Timeout Calculation**: Adaptive timeout mechanisms
- **Leader Coordination**: Manages leader timing and coordination
- **Network Adaptation**: Adjusts to network conditions

#### Validator Logic (`validator.rs`)
- **Block Validation**: Verifies proposed blocks for correctness
- **Transaction Verification**: Validates individual transactions
- **State Consistency**: Ensures state machine consistency
- **Execution Rules**: Defines valid state transitions

## 🔧 Integration Architecture

### Core Protocol Integration

```rust
use hotstuff2_core::{ProtocolState, SafetyRules, Pacemaker};

// Initialize core components
let mut protocol_state = ProtocolState::new(genesis_block)?;
let safety_rules = SafetyRules::new(validator_key)?;
let pacemaker = Pacemaker::new(timeout_config)?;

// Process consensus messages
let vote_decision = safety_rules.should_vote(&proposal, &protocol_state)?;
if vote_decision.should_vote {
    let vote = protocol_state.create_vote(proposal, validator_signature)?;
    consensus.broadcast_vote(vote).await?;
}
```

### Safety Rule Integration

```rust
impl SafetyRules {
    // Core safety check before voting
    fn should_vote(&self, proposal: &Proposal, state: &ProtocolState) -> VoteDecision {
        // Check if voting would violate safety
        if self.would_violate_safety(proposal, state) {
            return VoteDecision::abstain("safety violation");
        }
        
        // Verify proposal extends current locked block
        if !self.extends_locked_block(proposal, state) {
            return VoteDecision::abstain("doesn't extend locked block");
        }
        
        VoteDecision::vote()
    }
}
```

## 📊 Protocol Properties

### Safety Guarantees
- **Agreement**: No two honest nodes commit conflicting blocks
- **Validity**: Only valid blocks can be committed
- **Integrity**: Honest nodes' decisions cannot be forged

### Liveness Guarantees
- **Termination**: Consensus eventually terminates under synchrony
- **Progress**: New blocks are eventually proposed and committed
- **Responsiveness**: Fast decisions under optimal network conditions

### Performance Characteristics
- **State Overhead**: Minimal protocol state (O(1) per validator)
- **Message Validation**: Efficient cryptographic verification
- **Memory Usage**: Bounded state growth with garbage collection

## 🔒 Security Architecture

### Byzantine Fault Tolerance
- **Threat Model**: Up to f < n/3 Byzantine validators
- **Safety Preservation**: Maintains safety under all network conditions
- **Liveness Recovery**: Restores progress after network healing

### Cryptographic Integration
- **Message Authentication**: All protocol messages are signed
- **Hash Verification**: Block integrity through cryptographic hashes
- **Key Management**: Secure validator key handling

## 🛠️ Implementation Status

🚧 **Framework Phase**: Core protocol interfaces and architecture established.

### Completed Framework
✅ **Protocol Interfaces**: Well-defined APIs for all core components  
✅ **Safety Architecture**: Comprehensive safety rule framework  
✅ **State Management**: Clean state machine design  
✅ **Error Handling**: Robust error type system  

### Implementation Pipeline
🔄 **Safety Rules**: Detailed voting rule implementation  
🔄 **Pacemaker Logic**: Adaptive timeout mechanisms  
🔄 **State Persistence**: Reliable state storage  
🔄 **Validator Integration**: Block and transaction validation  

## 🔬 Academic Foundation

Implements core concepts from:

- **HotStuff** (Abraham et al., 2019) - Safety and liveness properties
- **PBFT** (Castro & Liskov, 1999) - Byzantine fault tolerance principles
- **FLP Impossibility** (Fischer et al., 1985) - Fundamental consensus limitations
- **CAP Theorem** (Brewer, 2000) - Consistency and availability trade-offs

## 📋 Module Dependencies

```toml
[dependencies]
hotstuff2-types = { path = "../types" }
hotstuff2-crypto = { path = "../crypto" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
```

## 🧪 Testing Strategy

### Property Testing
- Safety property verification under all conditions
- Liveness property testing under various network scenarios
- Byzantine behavior simulation

### Unit Testing
- Individual component validation
- State transition correctness
- Error handling verification

### Integration Testing
- Cross-module interaction validation
- End-to-end protocol flow testing
- Performance regression testing

---

**Design Philosophy**: This core module provides the foundational logic that ensures HotStuff-2's correctness properties while maintaining high performance and Byzantine fault tolerance. All components are designed for formal verification and rigorous testing.
