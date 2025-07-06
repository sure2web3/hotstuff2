# HotStuff-2 API Interface

**External HTTP/JSON-RPC interface for HotStuff-2 consensus nodes**, enabling applications, clients, and monitoring systems to interact with the consensus network through standardized protocols.

## 🎯 Role in HotStuff-2 Project

The API module serves as the **external interface and monitoring gateway** for HotStuff-2 consensus nodes, providing:

- **Consensus monitoring and observability** for network operators
- **Blockchain data access** for applications and block explorers  
- **Transaction submission** from external clients to the mempool
- **Network health monitoring** and diagnostic capabilities
- **Administrative operations** for node management and configuration

### Important: API Scope Clarification

**What the API provides:**
- ✅ **Read-only consensus monitoring** - Observe consensus health and progress
- ✅ **Transaction submission** - Submit transactions to mempool for inclusion
- ✅ **Blockchain data queries** - Access blocks, transactions, and network state
- ✅ **Network monitoring** - Track node status and network topology
- ✅ **Administrative functions** - Configure nodes and monitor system health

**What the API does NOT provide:**
- ❌ **Manual consensus control** - Block proposal and voting are automatic internal processes
- ❌ **Validator participation APIs** - Validators participate through the consensus protocol, not APIs
- ❌ **Consensus intervention** - View changes and QC formation happen automatically
- ❌ **Manual block creation** - Blocks are created automatically by the consensus algorithm

### HotStuff-2-Specific API Focus

**What HotStuff-2 APIs should prioritize:**
- ✅ **Transaction inclusion status** - Track transactions from pending → included → finalized
- ✅ **Consensus confirmations** - Number of subsequent blocks confirming transaction finality
- ✅ **Execution success/failure** - Whether transactions executed properly and their results
- ✅ **State changes** - What state modifications occurred as a result of execution
- ✅ **Performance metrics** - Execution time, resource usage, and consensus efficiency
- ✅ **Byzantine fault detection** - Safety violations and network health indicators
- ✅ **Finality guarantees** - Clear indication when transactions reach irreversible finality

**Less relevant for HotStuff-2:**
- ❌ **Gas fees and pricing** - Unless specifically implemented in the HotStuff-2 design
- ❌ **Mining-related metrics** - HotStuff-2 uses Byzantine fault-tolerant consensus, not PoW
- ❌ **Ethereum-specific fields** - EIP standards, uncle blocks, difficulty, etc.

## 🚀 Practical Use Cases in HotStuff-2

### 1. **Network Health Monitoring**

**Scenario**: Network operators monitoring HotStuff-2 consensus health

```bash
# Check overall network consensus status
curl -X GET "https://monitor.example.com:3000/api/v1/consensus/network-status" \
  -H "Authorization: Bearer <monitor-key>"
```

```json
{
  "block_height": 12345,
  "consensus_health": "HEALTHY",
  "active_validators": 7,
  "total_validators": 10,
  "current_view": 12345,
  "finalization_rate": 0.98,
  "average_block_time": 2.1,
  "last_finalized_block": {
    "height": 12344,
    "hash": "0x5678...",
    "timestamp": "2025-01-15T10:29:58Z"
  }
}
```

**What this enables:**
- Monitor overall network health and performance
- Track consensus progress and finalization rates
- Detect network issues or degraded performance
- Generate alerts for operational teams

### 2. **Transaction Submission and Tracking**

**Scenario**: Client applications submitting transactions through HotStuff-2 node APIs to the mempool

#### **A. Transaction Submission (curl example)**

```bash
# Submit transaction to node's mempool (will be included in consensus automatically)
curl -X POST "https://node.example.com:3000/api/v1/transactions" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <client-key>" \
  -d '{
    "from": "0x1234...",
    "to": "0x5678...",
    "value": "1000000",
    "data": "0xcontractcall...",
    "nonce": 42,
    "signature": "0xsignature..."
  }'
```

```json
{
  "transaction_hash": "0xabc123...",
  "status": "PENDING",
  "submitted_at": "2025-01-15T10:30:00Z"
}
```

#### **B. Transaction Status Tracking (curl example)**

```bash
# Track transaction status
curl -X GET "https://node.example.com:3000/api/v1/transactions/0xabc123..." \
  -H "Authorization: Bearer <client-key>"
```

```json
{
  "transaction_hash": "0xabc123...",
  "status": "FINALIZED",
  "block_height": 12346,
  "block_hash": "0x9876...",
  "confirmations": 3,
  "execution_result": {
    "success": true,
    "return_data": "0xresult...",
    "events": ["TransferEvent", "ApprovalEvent"]
  },
  "finalized_at": "2025-01-15T10:30:15Z"
}
```

#### **C. Programmatic Integration (JavaScript example)**

```javascript
// Submit transaction through API
async function submitTransaction(transaction) {
  const response = await fetch('https://node.example.com:3000/api/v1/transactions', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer <client-key>'
    },
    body: JSON.stringify({
      from: '0x1234...',
      to: '0x5678...',
      value: '1000000',
      data: '0xcontractcall...',
      nonce: 42,
      signature: '0xsignature...'
    })
  });
  
  const result = await response.json();
  return result.transaction_hash;
}

// Query transaction status
async function getTransactionStatus(txHash) {
  const response = await fetch(`https://node.example.com:3000/api/v1/transactions/${txHash}`);
  const status = await response.json();
  
  return {
    status: status.status, // PENDING, CONFIRMED, FINALIZED
    block_height: status.block_height,
    confirmations: status.confirmations
  };
}

// Complete transaction lifecycle monitoring
async function monitorTransaction(txHash) {
  let status = 'PENDING';
  
  while (status !== 'FINALIZED') {
    const txStatus = await getTransactionStatus(txHash);
    status = txStatus.status;
    
    console.log(`Transaction ${txHash}: ${status}`);
    
    if (status === 'FINALIZED') {
      console.log(`Finalized in block ${txStatus.block_height} with ${txStatus.confirmations} confirmations`);
      return txStatus;
    }
    
    // Wait before next check
    await new Promise(resolve => setTimeout(resolve, 2000));
  }
}
```

**What this enables:**
- Submit transactions to HotStuff-2 network mempool for consensus inclusion
- Track transaction lifecycle from submission to finalization
- Monitor transaction finalization and confirmations
- Query transaction execution results and state changes
- Integrate transaction submission into client applications programmatically

### 3. **Consensus Health Monitoring**

**Scenario**: Network operator monitoring consensus health and performance

#### **Option 1: HTTP Polling (Recommended for most use cases)**

```bash
# Regular health check polling (every 5-10 seconds)
curl -X GET "https://monitor.example.com:3000/api/v1/consensus/health" \
  -H "Authorization: Bearer <monitor-key>"
```

```json
{
  "timestamp": "2025-01-15T10:30:00Z",
  "consensus_health": "HEALTHY",
  "current_height": 12345,
  "finalization_rate": 0.98,
  "average_block_time": 2.1,
  "active_validators": 7,
  "recent_safety_violations": 0,
  "performance_metrics": {
    "tps": 150,
    "latency_ms": 1200,
    "memory_usage": "512MB"
  }
}
```

```javascript
// HTTP polling approach - Simple and reliable
class ConsensusMonitor {
  async startMonitoring() {
    setInterval(async () => {
      try {
        const health = await this.fetchConsensusHealth();
        this.updateDashboard(health);
        
        if (health.consensus_health !== 'HEALTHY') {
          this.triggerAlert(health);
        }
      } catch (error) {
        console.error('Monitoring failed:', error);
      }
    }, 5000); // Poll every 5 seconds
  }
}
```

#### **Option 2: WebSocket Streaming (For high-frequency events)**

```javascript
// WebSocket approach - For immediate event notifications
const ws = new WebSocket('wss://monitor.example.com:3000/api/v1/events');

ws.onmessage = (event) => {
  const consensusEvent = JSON.parse(event.data);
  
  switch(consensusEvent.type) {
    case 'BLOCK_FINALIZED':
      console.log(`Block ${consensusEvent.height} finalized`);
      updateBlockchainMetrics(consensusEvent);
      break;
    case 'SAFETY_ALERT':
      console.log(`Safety violation: ${consensusEvent.details}`);
      immediateAlert(consensusEvent); // Critical alerts need immediate action
      break;
    case 'VALIDATOR_OFFLINE':
      handleValidatorOffline(consensusEvent);
      break;
  }
};
```

#### **When to Use Each Approach:**

**HTTP Polling is better for:**
- ✅ **General health monitoring** - Check status every few seconds
- ✅ **Dashboard updates** - Regular metric updates are sufficient
- ✅ **Batch processing** - Collecting data for analysis
- ✅ **Simple implementation** - No connection management complexity
- ✅ **Firewall-friendly** - Standard HTTP works everywhere
- ✅ **Resource efficient** - Lower server resource usage

**WebSocket is better for:**
- ✅ **Critical alerts** - Byzantine behavior, safety violations
- ✅ **High-frequency events** - Block-by-block updates
- ✅ **Interactive applications** - Real-time user interfaces
- ✅ **Event-driven workflows** - Immediate reaction to consensus events
- ✅ **Low-latency requirements** - Sub-second response times for consensus events
- ✅ **Push-based notifications** - Server can immediately notify clients of consensus state changes
- ✅ **Reduced network overhead** - Single persistent connection vs repeated HTTP requests
- ✅ **Guaranteed event delivery** - Ordered, reliable stream of consensus events

**Why WebSocket is Preferred for Real-Time Consensus Monitoring:**

In Byzantine fault-tolerant consensus systems like HotStuff-2, **timing and immediate awareness of consensus events are critical** for maintaining network health and responding to issues:

1. **Byzantine Fault Detection**: Safety violations or Byzantine behavior must be detected and handled immediately to prevent network degradation. HTTP polling introduces delays (5-30 seconds) that could allow issues to compound.

2. **Consensus State Changes**: HotStuff-2 progresses through views and finality states rapidly. WebSocket ensures monitoring systems are notified instantly when:
   - View changes occur (leader rotation)
   - Blocks reach finality thresholds
   - Validator sets change
   - Network partitions are detected

3. **Resource Efficiency**: A single WebSocket connection can stream all consensus events, while HTTP polling requires separate requests for different metrics, creating unnecessary network traffic and server load.

4. **Event Ordering**: WebSocket provides ordered delivery of consensus events, ensuring monitoring systems see state changes in the correct sequence, which is crucial for accurate consensus analysis.

**Hybrid Approach (Recommended for Production):**

For production HotStuff-2 monitoring systems, use both approaches strategically:

```javascript
// Use HTTP polling for regular health checks and metrics collection
const healthMonitor = new HTTPHealthMonitor();
healthMonitor.startPolling(10000); // Every 10 seconds

// Use WebSocket only for critical event notifications that need immediate response
const alertStream = new WebSocket('wss://node.com/api/v1/critical-events');
alertStream.onmessage = handleCriticalEvent;

// Example: Comprehensive monitoring setup
class HotStuff-2Monitor {
  constructor() {
    this.httpPoller = new HTTPHealthMonitor();
    this.criticalEventStream = null;
  }

  async start() {
    // Start regular HTTP health monitoring
    this.httpPoller.startPolling(10000, (health) => {
      this.updateDashboard(health);
      this.logMetrics(health);
    });

    // Start WebSocket for critical real-time events
    this.criticalEventStream = new WebSocket('wss://node.example.com/api/v1/critical-events');
    this.criticalEventStream.onmessage = (event) => {
      const data = JSON.parse(event.data);
      
      switch(data.event_type) {
        case 'SAFETY_ALERT':
          this.handleByzantineBehavior(data);
          break;
        case 'CONSENSUS_STALL':
          this.handleConsensusStall(data);
          break;
        case 'VALIDATOR_OFFLINE':
          this.handleValidatorFailure(data);
          break;
      }
    };
  }

  handleByzantineBehavior(alert) {
    // Immediate response required - cannot wait for next HTTP poll
    this.triggerEmergencyAlert(alert);
    this.initiateIncidentResponse(alert);
  }
}
```

**Key Implementation Guidelines:**
- Use **HTTP polling** for metrics that can tolerate 5-30 second delays
- Use **WebSocket** only for events requiring immediate (sub-second) response
- Implement WebSocket reconnection logic for production reliability
- Use HTTP as fallback when WebSocket connections fail

#### **Technical Comparison: HTTP vs WebSocket for Consensus Monitoring**

| Aspect | HTTP Polling | WebSocket Streaming |
|--------|-------------|-------------------|
| **Latency** | 5-30 seconds (polling interval) | <100ms (immediate push) |
| **Network Overhead** | High (repeated headers, connections) | Low (single persistent connection) |
| **Server Resources** | Moderate (frequent requests) | Low (single connection per client) |
| **Event Ordering** | No guarantee | Guaranteed ordered delivery |
| **Connection Management** | Simple (stateless) | Complex (reconnection logic needed) |
| **Firewall Compatibility** | Excellent | Good (may need configuration) |
| **Use Case Fit** | Dashboard metrics, logs | Critical alerts, real-time UI |
| **Consensus Safety** | Delayed Byzantine detection | Immediate fault notification |
| **Implementation Complexity** | Low | Medium (connection handling) |

**For HotStuff-2 specifically**: WebSocket's immediate notification capability is crucial for detecting consensus safety violations, Byzantine behavior, and network partitions that could compromise the system's integrity.

### 4. **Network Administration and Monitoring**

**Scenario**: Node operator managing HotStuff-2 network

```bash
# Get network topology and peer status
curl -X GET "https://admin.example.com:3000/api/v1/network/peers" \
  -H "Authorization: Bearer <admin-key>"
```

```json
{
  "total_peers": 12,
  "connected_peers": 11,
  "validator_peers": 7,
  "peers": [
    {
      "peer_id": "peer-001",
      "address": "192.168.1.100:8080",
      "type": "validator",
      "status": "connected",
      "last_seen": "2025-01-15T10:29:55Z",
      "latency_ms": 45
    }
  ]
}
```

```bash
# Configure consensus parameters
curl -X PUT "https://admin.example.com:3000/api/v1/consensus/config" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <admin-key>" \
  -d '{
    "view_timeout_ms": 5000,
    "max_block_size": 1048576,
    "consensus_threshold": 0.67
  }'
```

**What this enables:**
- Monitor network connectivity and peer health
- Adjust consensus parameters for optimal performance
- Manage validator set changes and rotations
- Configure safety and liveness parameters

### 5. **Blockchain Data Access and Analytics**

**Scenario**: Block explorer querying blockchain data

```bash
# Get latest finalized block
curl -X GET "https://explorer.example.com:3000/api/v1/blocks/latest"

# Get block by height
curl -X GET "https://explorer.example.com:3000/api/v1/blocks/12345"

# Get validator set at specific height
curl -X GET "https://explorer.example.com:3000/api/v1/validators?height=12345"
```

```json
{
  "height": 12345,
  "hash": "0x1234...",
  "parent_hash": "0x5678...",
  "timestamp": "2025-01-15T10:30:00Z",
  "proposer": "validator-003",
  "qc": {
    "view": 12345,
    "signatures": ["0xsig1...", "0xsig2..."],
    "signature_bitmap": "0b11110111"
  },
  "transactions": [
    {
      "hash": "0xabc...",
      "from": "0x1111...",
      "to": "0x2222...",
      "value": "1000000"
    }
  ],
  "execution_result": {
    "transactions_executed": 150,
    "state_root": "0xstate...",
    "execution_time_ms": 45
  }
}
```

**What this enables:**
- Build blockchain explorers and analytics tools
- Query historical consensus data
- Analyze validator performance and behavior
- Extract consensus metrics and statistics

## 🔧 Core API Architecture

### HotStuff-2-Specific Endpoints

#### **Consensus Monitoring APIs**
```http
# Consensus state monitoring (read-only)
GET    /api/v1/consensus/status         # Current consensus state
GET    /api/v1/consensus/metrics        # Consensus performance metrics
GET    /api/v1/consensus/safety-report  # Safety violations and Byzantine behavior

# Network health monitoring
GET    /api/v1/network/validators       # Active validator set information
GET    /api/v1/network/topology         # Network topology and peer status
GET    /api/v1/metrics/network          # Network performance metrics
```

#### **Client and Application APIs**
```http
# Transaction operations  
POST   /api/v1/transactions            # Submit transaction to mempool
GET    /api/v1/transactions/{hash}     # Get transaction status and details
GET    /api/v1/mempool/status          # Get mempool statistics and pending transactions

# Blockchain data access
GET    /api/v1/blocks/{height}         # Get block by height
GET    /api/v1/blocks/{hash}           # Get block by hash
GET    /api/v1/blocks/latest           # Get latest finalized block
GET    /api/v1/accounts/{address}      # Get account state and balance
```

#### **Administrative APIs**
```http
# Node management
GET    /api/v1/node/info               # Node information and version
GET    /api/v1/node/health             # Node health status
PUT    /api/v1/node/config             # Update node configuration (admin only)

# System monitoring
GET    /api/v1/metrics/system          # System resource usage
GET    /api/v1/logs/consensus          # Consensus-related logs
GET    /api/v1/debug/state             # Debug information (admin only)
```

### JSON-RPC Methods for HotStuff-2

```javascript
// Query blockchain state and data
{
  "jsonrpc": "2.0",
  "method": "hotstuff2.getBlockByHeight",
  "params": {"height": 12345},
  "id": 1
}

// Submit transaction to network
{
  "jsonrpc": "2.0",
  "method": "hotstuff2.submitTransaction",
  "params": {
    "from": "0x1234...",
    "to": "0x5678...", 
    "value": "1000000",
    "data": "0xcontractcall..."
  },
  "id": 2
}

// Monitor consensus health (read-only)
{
  "jsonrpc": "2.0",
  "method": "hotstuff2.getConsensusStatus",
  "params": {},
  "id": 3
}

// Get network information
{
  "jsonrpc": "2.0",
  "method": "hotstuff2.getNetworkInfo", 
  "params": {},
  "id": 4
}
```

### WebSocket Event Streaming

Real-time monitoring of HotStuff-2 network events:

```javascript
// Subscribe to network events
const ws = new WebSocket('wss://node.example.com:3000/api/v1/events');

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  
  // Network monitoring events
  switch(data.event_type) {
    case 'BLOCK_FINALIZED':
      // Block reached finality - safe to rely on
      handleBlockFinalization(data.block);
      break;
      
    case 'TRANSACTION_CONFIRMED':
      // Transaction included in finalized block
      handleTransactionConfirmation(data.transaction);
      break;
      
    case 'CONSENSUS_HEALTH_CHANGE':
      // Consensus health status changed
      updateHealthStatus(data.status);
      break;
      
    case 'NETWORK_TOPOLOGY_CHANGE':
      // Network topology changed (peers joined/left)
      updateNetworkTopology(data.peers);
      break;
      
    case 'SAFETY_ALERT':
      // Safety-related alert (Byzantine behavior detected)
      handleSafetyAlert(data.alert);
      break;

    case 'PERFORMANCE_METRICS':
      // Regular performance metrics updates
      updateMetrics(data.metrics);
      break;
  }
};
```

## 🎯 Key Benefits for HotStuff-2 Ecosystem

1. **External Accessibility**: Easy client and application integration with HotStuff-2 network
2. **Network Transparency**: Real-time monitoring and analytics of consensus health
3. **Developer Experience**: Comprehensive APIs for building on HotStuff-2  
4. **Operational Excellence**: Production-ready monitoring and management tools
5. **Data Access**: Efficient blockchain data querying and transaction tracking
6. **Security First**: Built-in authentication, validation, and audit capabilities

This API module provides the essential external interface for HotStuff-2 consensus networks, enabling clients, applications, and operators to interact with the network while the consensus algorithm runs automatically and securely.