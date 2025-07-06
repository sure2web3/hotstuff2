# HotStuff-2 Metrics & Monitoring

**Comprehensive metrics and monitoring framework** designed for observability, performance analysis, and operational insights in HotStuff-2 consensus networks.

## 🎯 Design Philosophy

The HotStuff-2 metrics layer provides **comprehensive observability** into consensus operations, network performance, and system health with minimal overhead and maximum operational value.

### Core Principles

1. **Low-Overhead Collection**: Minimal performance impact on consensus operations
2. **Comprehensive Coverage**: Metrics for all critical consensus and network components
3. **Real-Time Insights**: Live dashboards and alerting for operational awareness
4. **Performance Analysis**: Deep dive capabilities for consensus optimization
5. **Standards Compliance**: Integration with standard monitoring ecosystems

## 🏗️ Architecture Overview

### Metrics Collection Architecture

```
┌─────────────────────────────────────────┐
│         HotStuff-2 Consensus             │
├─────────────────────────────────────────┤
│   Instrumentation & Data Collection     │  ← Performance Probes
├─────────────────────────────────────────┤
│ Metrics │ Tracing │ Logging │ Events  │  ← Observability Layers
│ Engine  │ System  │ Service │ Stream  │
├─────────────────────────────────────────┤
│ Prometheus │ Jaeger │ Grafana │ Custom │  ← Export & Visualization
│   Export   │ Export │ Dashbrd │ Export │
└─────────────────────────────────────────┘
```

## 🔧 Core Metrics Interface

### `MetricsCollector` Trait

**Purpose**: Unified interface for metrics collection across all HotStuff-2 components.

```rust
#[async_trait]
pub trait MetricsCollector: Send + Sync {
    // Counter Metrics
    async fn increment_counter(&self, name: &str, labels: &[(&str, &str)]);
    async fn increment_counter_by(&self, name: &str, value: u64, labels: &[(&str, &str)]);
    
    // Gauge Metrics
    async fn set_gauge(&self, name: &str, value: f64, labels: &[(&str, &str)]);
    async fn increment_gauge(&self, name: &str, delta: f64, labels: &[(&str, &str)]);
    
    // Histogram Metrics
    async fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]);
    async fn time_operation<F, R>(&self, name: &str, labels: &[(&str, &str)], operation: F) -> R
    where
        F: FnOnce() -> R;
    
    // Custom Metrics
    async fn record_custom_metric(&self, metric: CustomMetric);
    async fn flush_metrics(&self) -> MetricsResult<()>;
}
```

**Key Design Decisions**:
- **Label-based organization**: Efficient metric organization with labels
- **Async collection**: Non-blocking metrics recording
- **Timing utilities**: Built-in operation timing support
- **Custom metric support**: Extensible for domain-specific metrics

## 📊 Consensus Metrics

### Core Consensus Performance

```rust
pub struct ConsensusMetrics {
    // Block Processing Metrics
    blocks_proposed: Counter,
    blocks_committed: Counter,
    block_proposal_time: Histogram,
    block_commit_time: Histogram,
    
    // View Management Metrics
    current_view: Gauge,
    view_changes: Counter,
    view_change_duration: Histogram,
    
    // Vote Metrics
    votes_sent: Counter,
    votes_received: Counter,
    vote_processing_time: Histogram,
    
    // Safety & Liveness Metrics
    safety_violations: Counter,
    liveness_timeouts: Counter,
    byzantine_behavior_detected: Counter,
}

impl ConsensusMetrics {
    // Block Metrics
    pub fn record_block_proposed(&self, height: u64, view: u64, tx_count: usize);
    pub fn record_block_committed(&self, height: u64, view: u64, latency: Duration);
    pub fn record_block_proposal_time(&self, duration: Duration);
    
    // View Metrics
    pub fn record_view_change(&self, from_view: u64, to_view: u64, reason: ViewChangeReason);
    pub fn update_current_view(&self, view: u64);
    
    // Vote Metrics
    pub fn record_vote_sent(&self, vote_type: VoteType, view: u64);
    pub fn record_vote_received(&self, vote_type: VoteType, view: u64, validator: &ValidatorId);
    
    // Safety Metrics
    pub fn record_safety_violation(&self, violation_type: SafetyViolationType, view: u64);
    pub fn record_byzantine_behavior(&self, behavior_type: ByzantineBehaviorType, validator: &ValidatorId);
}
```

### Performance Analysis Metrics

```rust
pub struct PerformanceMetrics {
    // Throughput Metrics
    transaction_throughput: Gauge,
    block_throughput: Gauge,
    consensus_throughput: Gauge,
    
    // Latency Metrics
    transaction_confirmation_latency: Histogram,
    block_finalization_latency: Histogram,
    end_to_end_latency: Histogram,
    
    // Resource Usage
    cpu_usage: Gauge,
    memory_usage: Gauge,
    disk_usage: Gauge,
    network_bandwidth: Gauge,
    
    // Efficiency Metrics
    consensus_efficiency: Gauge,
    network_efficiency: Gauge,
    storage_efficiency: Gauge,
}
```

## 🌐 Network Metrics

### Communication Performance

```rust
pub struct NetworkMetrics {
    // Message Metrics
    messages_sent: Counter,
    messages_received: Counter,
    message_processing_time: Histogram,
    message_queue_size: Gauge,
    
    // Peer Management
    active_peers: Gauge,
    peer_connections: Counter,
    peer_disconnections: Counter,
    peer_latency: Histogram,
    
    // Bandwidth Metrics
    bytes_sent: Counter,
    bytes_received: Counter,
    bandwidth_utilization: Gauge,
    
    // Network Health
    network_partitions: Counter,
    connection_failures: Counter,
    message_delivery_rate: Gauge,
}

impl NetworkMetrics {
    // Message Tracking
    pub fn record_message_sent(&self, msg_type: MessageType, peer: &PeerId, size: usize);
    pub fn record_message_received(&self, msg_type: MessageType, peer: &PeerId, size: usize);
    pub fn record_message_processing_time(&self, msg_type: MessageType, duration: Duration);
    
    // Peer Management
    pub fn record_peer_connected(&self, peer: &PeerId, connection_type: ConnectionType);
    pub fn record_peer_disconnected(&self, peer: &PeerId, reason: DisconnectionReason);
    pub fn update_peer_count(&self, count: usize);
    
    // Network Health
    pub fn record_network_partition(&self, affected_peers: usize);
    pub fn record_connection_failure(&self, peer: &PeerId, error: &NetworkError);
}
```

## 🗄️ Storage & State Metrics

### Storage Performance

```rust
pub struct StorageMetrics {
    // Read/Write Operations
    storage_reads: Counter,
    storage_writes: Counter,
    storage_deletes: Counter,
    
    // Performance
    read_latency: Histogram,
    write_latency: Histogram,
    batch_operation_latency: Histogram,
    
    // Storage Health
    storage_size: Gauge,
    storage_utilization: Gauge,
    storage_errors: Counter,
    
    // Cache Performance
    cache_hits: Counter,
    cache_misses: Counter,
    cache_size: Gauge,
}

impl StorageMetrics {
    pub fn record_storage_read(&self, key_type: KeyType, duration: Duration, success: bool);
    pub fn record_storage_write(&self, key_type: KeyType, size: usize, duration: Duration);
    pub fn record_batch_operation(&self, operation_count: usize, duration: Duration);
    pub fn update_storage_size(&self, size_bytes: u64);
}
```

### State Management Metrics

```rust
pub struct StateMetrics {
    // State Operations
    state_transitions: Counter,
    state_queries: Counter,
    state_snapshots: Counter,
    
    // Performance
    state_transition_time: Histogram,
    state_query_time: Histogram,
    snapshot_creation_time: Histogram,
    
    // State Health
    state_size: Gauge,
    account_count: Gauge,
    contract_count: Gauge,
    
    // Validation
    transaction_validations: Counter,
    validation_failures: Counter,
    validation_time: Histogram,
}
```

## 🔍 Monitoring & Alerting

### Alert Configuration

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlertConfig {
    // Performance Alerts
    pub max_block_time: Duration,
    pub max_view_change_frequency: f64,
    pub min_transaction_throughput: f64,
    
    // Network Alerts  
    pub max_peer_disconnection_rate: f64,
    pub max_message_loss_rate: f64,
    pub max_network_latency: Duration,
    
    // Resource Alerts
    pub max_cpu_usage: f64,
    pub max_memory_usage: f64,
    pub max_disk_usage: f64,
    
    // Consensus Alerts
    pub max_byzantine_behavior_rate: f64,
    pub max_safety_violations: u64,
    pub max_liveness_timeouts: u64,
}

pub struct AlertManager {
    config: AlertConfig,
    alert_handlers: Vec<Box<dyn AlertHandler>>,
}

impl AlertManager {
    pub async fn check_consensus_health(&self, metrics: &ConsensusMetrics) -> Vec<Alert>;
    pub async fn check_network_health(&self, metrics: &NetworkMetrics) -> Vec<Alert>;
    pub async fn check_system_health(&self, metrics: &SystemMetrics) -> Vec<Alert>;
    
    pub async fn send_alert(&self, alert: Alert);
    pub async fn resolve_alert(&self, alert_id: &str);
}
```

### Health Checks

```rust
pub struct HealthChecker {
    consensus_health: Box<dyn ConsensusHealthChecker>,
    network_health: Box<dyn NetworkHealthChecker>,
    storage_health: Box<dyn StorageHealthChecker>,
}

impl HealthChecker {
    pub async fn overall_health_status(&self) -> HealthStatus;
    pub async fn detailed_health_report(&self) -> HealthReport;
    pub async fn component_health(&self, component: ComponentType) -> ComponentHealth;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning { issues: Vec<HealthIssue> },
    Critical { issues: Vec<HealthIssue> },
    Failing { issues: Vec<HealthIssue> },
}
```

## 📈 Performance Dashboards

### Grafana Dashboard Integration

```rust
pub struct DashboardConfig {
    // Consensus Performance Dashboard
    pub consensus_dashboard: ConsensusDashboardConfig,
    
    // Network Performance Dashboard
    pub network_dashboard: NetworkDashboardConfig,
    
    // System Resource Dashboard
    pub system_dashboard: SystemDashboardConfig,
    
    // Custom Dashboard Support
    pub custom_dashboards: Vec<CustomDashboardConfig>,
}

pub struct DashboardExporter {
    prometheus_client: PrometheusClient,
    grafana_client: GrafanaClient,
}

impl DashboardExporter {
    pub async fn export_consensus_dashboard(&self) -> DashboardResult<String>;
    pub async fn export_network_dashboard(&self) -> DashboardResult<String>;
    pub async fn create_custom_dashboard(&self, config: &CustomDashboardConfig) -> DashboardResult<String>;
}
```

## 🔧 Export & Integration

### Prometheus Integration

```rust
pub struct PrometheusExporter {
    registry: prometheus::Registry,
    port: u16,
    metrics_prefix: String,
}

impl PrometheusExporter {
    pub fn new(port: u16, prefix: String) -> Self;
    pub async fn start_server(&self) -> MetricsResult<()>;
    pub fn register_metrics(&self, metrics: &dyn MetricsCollector) -> MetricsResult<()>;
    pub async fn scrape_metrics(&self) -> String;
}
```

### Custom Export Formats

```rust
pub trait MetricsExporter: Send + Sync {
    async fn export_metrics(&self, metrics: &MetricsSnapshot) -> ExportResult<Vec<u8>>;
    fn format_name(&self) -> &'static str;
}

// JSON Export
pub struct JsonExporter;

// CSV Export  
pub struct CsvExporter;

// InfluxDB Export
pub struct InfluxDbExporter {
    client: InfluxDbClient,
    database: String,
}
```

## 🧪 Testing Framework

### Metrics Testing Utilities

```rust
pub mod test_utils {
    pub fn create_test_metrics_collector() -> TestMetricsCollector;
    pub fn simulate_consensus_load(metrics: &dyn MetricsCollector, duration: Duration);
    pub fn simulate_network_conditions(metrics: &NetworkMetrics, conditions: NetworkConditions);
    pub async fn assert_metrics_recorded(collector: &TestMetricsCollector, expected: &[ExpectedMetric]);
}

pub struct MetricsTestFramework {
    collector: TestMetricsCollector,
    scenario_runner: ScenarioRunner,
}

impl MetricsTestFramework {
    // Load Testing
    pub async fn test_high_throughput_metrics(&self, tps: u64, duration: Duration);
    pub async fn test_large_scale_network_metrics(&self, peer_count: usize);
    
    // Performance Testing
    pub async fn benchmark_metrics_collection_overhead(&self) -> BenchmarkResults;
    pub async fn test_metrics_accuracy(&self, known_workload: TestWorkload);
    
    // Integration Testing
    pub async fn test_prometheus_export(&self);
    pub async fn test_grafana_dashboard_generation(&self);
}
```

## 📊 Performance Characteristics

### Target Performance

- **Metrics collection overhead**: < 1% of consensus performance
- **Metrics export**: < 100ms for full snapshot
- **Dashboard refresh**: < 5s for live dashboards
- **Alert response time**: < 1s for critical alerts
- **Storage overhead**: < 100MB for 24h of metrics

### Optimization Techniques

- **Batch collection**: Group metrics collection for efficiency
- **Sampling**: Sample high-frequency events to reduce overhead
- **Compression**: Compress historical metrics data
- **Lazy aggregation**: Compute complex metrics on-demand

## 🔧 Configuration

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MetricsConfig {
    // Collection Configuration
    pub collection_enabled: bool,
    pub collection_interval: Duration,
    pub max_metrics_memory: usize,
    
    // Export Configuration
    pub prometheus_enabled: bool,
    pub prometheus_port: u16,
    pub custom_exporters: Vec<ExporterConfig>,
    
    // Dashboard Configuration
    pub grafana_integration: bool,
    pub dashboard_auto_generation: bool,
    pub custom_dashboards: Vec<String>,
    
    // Alerting Configuration
    pub alerting_enabled: bool,
    pub alert_config: AlertConfig,
    pub alert_destinations: Vec<AlertDestination>,
    
    // Performance Tuning
    pub metrics_sampling_rate: f64,
    pub batch_size: usize,
    pub async_export: bool,
}
```

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module contains interface definitions, architectural design, and integration points for the HotStuff-2 metrics and monitoring system.

**Current State**: 
- ✅ Metrics collection interface design
- ✅ Observability architecture planning
- ✅ Export and integration framework
- ✅ Dashboard and alerting design
- ⏳ Implementation pending

## 🔬 Academic Foundation

Metrics and monitoring design based on proven observability practices:

- **Prometheus**: Industry-standard metrics collection and storage
- **Grafana**: Comprehensive visualization and dashboard framework
- **Jaeger**: Distributed tracing for consensus protocol analysis
- **OpenTelemetry**: Unified observability standards
- **SRE Practices**: Site Reliability Engineering monitoring methodologies

The design emphasizes **operational excellence**, **performance transparency**, and **proactive monitoring** while maintaining minimal overhead on consensus operations.

## 🔗 Integration Points

- **consensus/**: Core consensus metrics and performance monitoring
- **network/**: Network communication and peer management metrics
- **storage/**: Storage performance and health monitoring
- **validator/**: Validator performance and reputation tracking
- **mempool/**: Transaction pool metrics and throughput monitoring
