# HotStuff-2 Utilities

**Common utility functions and helper modules** designed to provide shared functionality across all HotStuff-2 components.

## 🎯 Design Philosophy

The HotStuff-2 utilities provide **reusable, well-tested components** that eliminate code duplication and provide consistent functionality across the entire consensus system.

### Core Principles

1. **Code Reusability**: Eliminate duplication across modules
2. **High Quality**: Well-tested, reliable utility functions
3. **Performance**: Optimized implementations of common operations
4. **Consistency**: Uniform behavior across all system components
5. **Minimal Dependencies**: Lightweight utilities with minimal external dependencies

## 🏗️ Architecture Overview

### Utility Categories

```
┌─────────────────────────────────────────┐
│         HotStuff-2 Components            │
├─────────────────────────────────────────┤
│         Shared Utility Layer            │  ← Common Functions
├─────────────────────────────────────────┤
│ Crypto  │ Network │ Storage │ Time    │  ← Utility Domains
│ Utils   │ Utils   │ Utils   │ Utils   │
├─────────────────────────────────────────┤
│ Config  │ Logging │ Testing │ Math    │  ← Supporting Utilities
│ Utils   │ Utils   │ Utils   │ Utils   │
└─────────────────────────────────────────┘
```

## 🔧 Core Utility Modules

### Configuration Utilities

```rust
pub mod config {
    // Configuration Loading
    pub fn load_config_from_file<T: serde::de::DeserializeOwned>(path: &Path) -> UtilResult<T>;
    pub fn load_config_from_env<T: serde::de::DeserializeOwned>(prefix: &str) -> UtilResult<T>;
    pub fn merge_configs<T>(base: T, override_config: T) -> UtilResult<T>;
    
    // Configuration Validation
    pub fn validate_config<T: Validate>(config: &T) -> ValidationResult;
    pub fn normalize_config_paths(config: &mut dyn ConfigWithPaths) -> UtilResult<()>;
}
```

### Time Utilities

```rust
pub mod time {
    // Time Operations
    pub fn current_timestamp() -> u64;
    pub fn current_timestamp_millis() -> u64;
    pub fn timestamp_to_datetime(timestamp: u64) -> DateTime<Utc>;
    
    // Duration Utilities
    pub fn parse_duration(s: &str) -> UtilResult<Duration>;
    pub fn format_duration(duration: Duration) -> String;
    
    // Timeout Utilities
    pub async fn with_timeout<F, T>(future: F, timeout: Duration) -> TimeoutResult<T>
    where F: Future<Output = T>;
}
```

### Encoding Utilities

```rust
pub mod encoding {
    // Hex Encoding
    pub fn to_hex(data: &[u8]) -> String;
    pub fn from_hex(hex_str: &str) -> UtilResult<Vec<u8>>;
    
    // Base64 Encoding
    pub fn to_base64(data: &[u8]) -> String;
    pub fn from_base64(base64_str: &str) -> UtilResult<Vec<u8>>;
    
    // Binary Serialization
    pub fn serialize_to_bytes<T: serde::Serialize>(value: &T) -> UtilResult<Vec<u8>>;
    pub fn deserialize_from_bytes<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> UtilResult<T>;
}
```

## 🛠️ Implementation Status

🚧 **Framework Phase**: This module contains interface definitions and architectural design for HotStuff-2 utility functions.

**Current State**: 
- ✅ Utility module design
- ✅ Common function interfaces
- ⏳ Implementation pending

## 🔗 Integration Points

- **All modules**: Shared utility functions used throughout the system
- **config/**: Configuration management utilities
- **crypto/**: Cryptographic utility functions
- **network/**: Network utility functions
