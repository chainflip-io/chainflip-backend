use crate::common::Block;
use crossbeam_channel::Receiver;

/// Loki RPC wallet API
pub mod loki_rpc;

/// Connects to loki rpc wallet and pushes payments to the witness
pub struct LokiConnection {
    config: LokiConnectionConfig,
    last_block: u64,
}

pub struct LokiConnectionConfig {
    pub rpc_wallet_port: u16, // 6934
}

impl LokiConnection {
    /// Default implementation
    pub fn new(config: LokiConnectionConfig) -> LokiConnection {
        // Use some recent block hieght for now
        let last_block = 363680;
        LokiConnection { config, last_block }
    }

    /// Poll loki rpc wallet
    async fn poll_once(&mut self) -> Result<(), String> {
        let cur_height = loki_rpc::get_height().await?;

        if cur_height == self.last_block {
            return Ok(());
        }

        let next_block = self.last_block + 1;

        debug!(
            "Requesting payments from blocks in [{}; {}]",
            next_block, cur_height
        );

        // Should I only request new payments when there is a new block?
        let res = loki_rpc::get_bulk_payments(vec![], next_block).await?;

        for payment in res.payments {
            debug!("payment: {:?}", payment);
        }

        // For now we only update when we see a new block
        // (since the response does not give us the lastest block height)
        // (TODO: use another endpoint `get_height`)
        self.last_block = cur_height;

        Ok(())
    }

    async fn poll_loop(mut self, tx: crossbeam_channel::Sender<Block>) {
        loop {
            // Simply wait for next iteration in case of errors
            let _ = self.poll_once().await;

            tx.send(Block { txs: vec![] }).unwrap();

            tokio::time::delay_for(std::time::Duration::from_millis(2000)).await;
        }
    }

    /// Start polling the blockchain in a separate thread
    pub fn start(self) -> Receiver<Block> {
        let (tx, rx) = crossbeam_channel::unbounded::<Block>();

        // We can't use block_on directly, because that would block
        // a function that might potentially be called from an `async`
        // function which should not be allowed to blocked. We
        // spawn a separate thread to make it non-blocking.

        std::thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(self.poll_loop(tx));
        });

        rx
    }
}
