use clap::{Arg, Command};
use hotstuff2::node::{Node, NodeConfig};
use std::net::SocketAddr;
use std::str::FromStr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse command line arguments
    let matches = Command::new("HotStuff-2 Node")
        .version("0.1.0")
        .about("Runs a node in the HotStuff-2 consensus network")
        .arg(
            Arg::new("id")
                .short('i')
                .long("id")
                .value_name("NODE_ID")
                .help("Node identifier")
                .required(true),
        )
        .arg(
            Arg::new("listen")
                .short('l')
                .long("listen")
                .value_name("ADDRESS")
                .help("Listen address (e.g., 127.0.0.1:8000)")
                .required(true),
        )
        .arg(
            Arg::new("peers")
                .short('p')
                .long("peers")
                .value_name("PEERS")
                .help("Comma-separated list of peer addresses (id@ip:port)")
                .required(true),
        )
        .arg(
            Arg::new("data-dir")
                .long("data-dir")
                .value_name("DATA_DIR")
                .help("Directory for node data (e.g., ./data/node0)")
                .required(false)
                .default_value("./data"),
        )
        .arg(
            Arg::new("metrics")
                .long("metrics")
                .value_name("METRICS_PORT")
                .help("Enable metrics server on specified port"),
        )
        .get_matches();

    // Extract node ID
    let node_id = matches
        .get_one::<String>("id")
        .unwrap()
        .parse::<u64>()
        .expect("Invalid node ID");

    // Extract listen address
    let listen_addr = matches
        .get_one::<String>("listen")
        .unwrap()
        .parse::<SocketAddr>()
        .expect("Invalid listen address");

    // Parse peer addresses
    let peers_str = matches.get_one::<String>("peers").unwrap();
    let peers = peers_str
        .split(',')
        .filter_map(|peer| {
            let parts: Vec<&str> = peer.split('@').collect();
            if parts.len() != 2 {
                eprintln!("Invalid peer format: {}", peer);
                return None;
            }
            let id = parts[0].parse::<u64>().ok()?;
            let addr = SocketAddr::from_str(parts[1]).ok()?;
            Some((id, addr.to_string()))
        })
        .collect();

    let data_dir = matches.get_one::<String>("data-dir").unwrap().clone();

    // Create node configuration
    let mut config = NodeConfig::default();
    config.node_id = node_id;
    config.listen_addr = listen_addr.to_string();
    config.peers = peers;
    config.data_dir = data_dir;

    // Create and start the node
    let mut node = Node::new(config);
    node.start().await.expect("Failed to start node");

    // Keep the main thread alive
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");

    // Gracefully shutdown the node
    node.stop().await.expect("Failed to stop node");

    Ok(())
}
