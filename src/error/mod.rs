use thiserror::Error;

#[derive(Error, Debug)]
pub enum HotStuffError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Timer error: {0}")]
    Timer(String),

    #[error("Node already started")]
    AlreadyStarted,

    #[error("Node not running")]
    NotRunning,

    #[error("Consensus error: {0}")]
    Consensus(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Metrics error: {0}")]
    Metrics(String),

    #[error("Invalid threshold: {0} out of {1} total nodes")]
    InvalidThreshold(usize, usize),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),

    #[error("Threshold signature combination failed")]
    ThresholdCombinationFailed,
}
