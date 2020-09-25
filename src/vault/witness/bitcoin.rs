/// Work in Progress

const START_BLOCK: u64 = 647705;

/// The db key for fetching and storing the next BTC block
const NEXT_BTC_BLOCK_KEY: &'static str = "next_btc_block";

/// A Bitcoin transaction witness
pub struct BitcoinWitness<T, C, S>
where
    T: TransactionProvider,
    C: BitcoinClient,
    S: KeyValueStore,
{
    transaction_provider: Arc<Mutex<T>>,
    client: Arc<C>,
    store: Arc<Mutex<S>>,
    next_bitcoin_block: u64,
}

/// How much of this code can be shared between chains??
impl<T, C, S> BitcoinWitness<T, C, S>
where
    T: TransactionProvider + Send + 'static,
    C: BitcoinClient + Send + Sync + 'static,
    S: KeyValueStore + Send + 'static,
{
    /// Create a new bitcoin chain witness
    pub fn new(client: Arc<C>, transaction_provider: Arc<Mutex<T>>, store: Arc<Mutex<S>>) -> Self {
        let next_bitcoin_block = match store.lock().unwrap().get_data::<u64>(NEXT_BTC_BLOCK_KEY) {
            Some(next_block) => next_block,
            None => {
                warn!(
                    "Last block record not found for BTC witness, using default: {}",
                    START_BLOCK
                );
                START_BLOCK
            }
        };

        BitcoinWitness {
            client,
            transaction_provider,
            store,
            next_bitcoin_block,
        }
    }
}
