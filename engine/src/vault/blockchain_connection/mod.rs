//! Oxen payments don't always appear in the latest block,
//! so we poll for a sliding window of size SLIDING_WINDOW_SIZE with
//! `last_processed_block` being the latest block that
//! will not be requested again

use crate::common::store::{KeyValueStore, PersistentKVS};
use crossbeam_channel::Receiver;

/// Oxen RPC wallet API
pub mod oxen_rpc;

/// Ethereum API
pub mod ethereum;

pub use ethereum::web3::Web3Client;

/// Bitcoin API
pub mod btc;
pub use btc::spv::BtcSPVClient;

/// Connects to oxen rpc wallet and pushes payments to the witness
pub struct OxenConnection {
    config: OxenConnectionConfig,
    /// database for persistent state
    db: PersistentKVS,
    last_processed_block: u64,
}

/// Configuration for oxen wallet connection
pub struct OxenConnectionConfig {
    /// port on which oxen wallet rpc is running
    pub rpc_wallet_port: u16,
}

/// Payment for now is just an alias to the type returned
/// directly from the wallet, but might turn it into its
/// own struct in the future
pub type Payment = oxen_rpc::BulkPaymentResponseEntry;

/// Convenience alias for a vector of payments
pub type Payments = Vec<Payment>;

/// Oxen connection start scanning from this block if no can't
/// find existing database records. (There is no reason to scan
/// blocks that came before chainflip launch)
const FIRST_CHAINFLIP_BLOCK: u64 = 363680;

/// Oxen connection requests payments for a sliding window of blocks
/// because the wallet does not (always) acknowledge payments right away
const SLIDING_WINDOW_SIZE: u64 = 2;

const LAST_PROCESSED_DB_KEY: &'static str = "last_processed_block";

impl OxenConnection {
    /// Default implementation
    pub fn new(config: OxenConnectionConfig) -> OxenConnection {
        // Use some recent block hieght for now

        let connection =
            rusqlite::Connection::open("oxen_connection").expect("Failed to open connection");

        let db = PersistentKVS::new(connection);

        let last_processed_block = match db.get_data::<u64>(LAST_PROCESSED_DB_KEY) {
            Some(last_block) => {
                info!(
                    "Loaded last block record for Oxen Connection from DB: {}",
                    last_block
                );
                last_block
            }
            None => {
                warn!(
                    "Last block record not found for Oxen Connection, using default: {}",
                    FIRST_CHAINFLIP_BLOCK
                );
                FIRST_CHAINFLIP_BLOCK
            }
        };

        OxenConnection {
            config,
            db,
            last_processed_block,
        }
    }

    /// Poll oxen rpc wallet
    async fn poll_once(&mut self) -> Result<Payments, String> {
        let cur_height = oxen_rpc::get_height(self.config.rpc_wallet_port).await?;

        // We only request payments when the blockchain made progress,
        // i.e. cur_height >= last_processed_block + SLIDING_WINDOW_SIZE

        // We can safely assume that the blockchain has more than SLIDING_WINDOW_SIZE,
        // blocks, so we can maintain the invariant that the first block that we
        // haven't requested yet is self.last_processed_block + SLIDING_WINDOW_SIZE.
        let first_unseen = self.last_processed_block + SLIDING_WINDOW_SIZE;

        let start_block = self.last_processed_block + 1;

        if cur_height < first_unseen {
            return Ok(vec![]);
        }

        info!("New oxen blockchain height is {}", cur_height);

        info!(
            "Requesting payments from blocks in [{}; {}] ({} blocks)",
            start_block,
            cur_height,
            (cur_height - start_block + 1)
        );

        let res =
            oxen_rpc::get_bulk_payments(self.config.rpc_wallet_port, vec![], start_block).await?;

        // IMPORTANT: it might be too early to mark the block as processed (since the
        // program can be interrupted before the handler had the chance
        // to process them), so we might need to wait for an acknowledgement
        // from the witness. (Although our sliding window processing can mitigate that.)

        self.last_processed_block = cur_height.saturating_sub(SLIDING_WINDOW_SIZE - 1);
        self.db
            .set_data(LAST_PROCESSED_DB_KEY, Some(self.last_processed_block))
            .expect("Could not update last processed block entry in the database");

        info!("Last processed block is now: {}", self.last_processed_block);

        Ok(res.payments)
    }

    async fn poll_loop(mut self, channel: crossbeam_channel::Sender<Payments>) {
        loop {
            // Simply wait for next iteration in case of errors
            if let Ok(payments) = self.poll_once().await {
                // TODO: if SLIDING_WINDOW_SIZE > 2, we need to make sure that we don't
                // send payments that we have seen already

                if payments.len() > 0 {
                    channel.send(payments).unwrap();
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
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
            let rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(self.poll_loop(tx));
        });

        rx
    }
}
