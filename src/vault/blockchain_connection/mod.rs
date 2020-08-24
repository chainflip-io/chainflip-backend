use crate::common::Block;
use crossbeam_channel::Receiver;

/// Loki RPC wallet API
pub mod loki_rpc;

/// Ethereum API
pub mod ethereum;

// Connects to lokid can pushes tx to the witness
pub struct LokiConnection {}

impl LokiConnection {
    /// Default implementation
    pub fn new() -> LokiConnection {
        LokiConnection {}
    }

    /// Start polling the blockchain in a separate thread
    pub fn start(self) -> Receiver<Block> {
        let (tx, rx) = crossbeam_channel::unbounded::<Block>();

        std::thread::spawn(move || {
            loop {
                // For now we create fake block;

                let b = Block { txs: vec![] };

                debug!("Loki connection: {:?}", &b);

                if tx.send(b).is_err() {
                    error!("Could not send block");
                }

                std::thread::sleep(std::time::Duration::from_millis(2000));
            }
        });

        rx
    }
}
