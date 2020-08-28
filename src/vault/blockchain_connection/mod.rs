use crossbeam_channel::Receiver;

/// Loki RPC wallet API
pub mod loki_rpc;

/// Ethereum API
pub mod ethereum;

/// Connects to loki rpc wallet and pushes payments to the witness
pub struct LokiConnection {
    config: LokiConnectionConfig,
    last_block: u64,
}

/// Configuration for loki wallet connection
pub struct LokiConnectionConfig {
    /// port on which loki wallet rpc is running
    pub rpc_wallet_port: u16,
}

/// Payment for now is just an alias to the type returned
/// directly from the wallet, but might turn it into its
/// own struct in the future
pub type Payment = loki_rpc::BulkPaymentResponseEntry;

/// Convenience alias for a vector of payments
pub type Payments = Vec<Payment>;

impl LokiConnection {
    /// Default implementation
    pub fn new(config: LokiConnectionConfig) -> LokiConnection {
        // Use some recent block hieght for now
        let last_block = 363680;
        LokiConnection { config, last_block }
    }

    /// Poll loki rpc wallet
    async fn poll_once(&mut self) -> Result<Payments, String> {
        let cur_height = loki_rpc::get_height(self.config.rpc_wallet_port).await?;

        // We only start looking at blocks when the are 1 block old,
        // i.e. the invariant is self.last_block <= cur_height - 1

        if cur_height == self.last_block + 1 {
            return Ok(vec![]);
        }

        let next_block = self.last_block + 1;

        info!("Current height is {}", cur_height);

        info!(
            "Requesting payments from blocks in [{}; {}] ({} blocks)",
            next_block,
            cur_height,
            (cur_height - next_block + 1)
        );

        let res =
            loki_rpc::get_bulk_payments(self.config.rpc_wallet_port, vec![], next_block).await?;

        // For now we only update when we see a new block
        // (since the response does not give us the lastest block height)
        // (TODO: use another endpoint `get_height`)
        self.last_block = cur_height.saturating_sub(1);

        Ok(res.payments)
    }

    async fn poll_loop(mut self, tx: crossbeam_channel::Sender<Payments>) {
        loop {
            // Simply wait for next iteration in case of errors
            if let Ok(payments) = self.poll_once().await {
                if payments.len() > 0 {
                    tx.send(payments).unwrap();
                }
            }

            tokio::time::delay_for(std::time::Duration::from_millis(2000)).await;
        }
    }

    /// Start polling the blockchain in a separate thread
    pub fn start(self) -> Receiver<Payments> {
        let (tx, rx) = crossbeam_channel::unbounded::<Payments>();

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
