use crate::{
    common::{store::KeyValueStore, Coin, Timestamp, WalletAddress},
    side_chain::SideChainTx,
    transactions::WitnessTx,
    vault::{blockchain_connection::btc::BitcoinClient, transactions::TransactionProvider},
};

use bitcoin::blockdata::transaction::*;
use bitcoin::Address;
use uuid::Uuid;

use std::sync::{Arc, Mutex};

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

    async fn event_loop(&mut self) {
        loop {
            self.poll_next_main_chain().await;

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Start witnessing the bitcoin chain on a new thread
    pub async fn start(mut self) {
        std::thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                self.event_loop().await;
            });
        });
    }

    fn addresses_from_output_pubkeys(&self, output: &Vec<TxOut>) -> Vec<WalletAddress> {
        let mut addresses: Vec<WalletAddress> = vec![];
        for txout in output {
            let address =
                match Address::from_script(&txout.script_pubkey, self.client.get_network_type()) {
                    Some(address) => address,
                    None => {
                        error!("Invalid bitoin script address, {:?}", &txout.script_pubkey);
                        continue;
                    }
                };
            addresses.push(WalletAddress::new(&address.to_string()));
        }
        addresses
    }

    async fn poll_next_main_chain(&mut self) {
        while let Some(txs) = self.client.get_transactions(self.next_bitcoin_block).await {
            let mut provider = self.transaction_provider.lock().unwrap();
            provider.sync();
            let quotes = provider.get_quote_txs();

            let mut witness_txs: Vec<WitnessTx> = vec![];
            for (tx_index, tx) in txs.iter().enumerate() {
                let tx_output = &tx.output;

                // btc has multiple output addresses (normally an Out and a Change address)
                // for a single transaction
                let btc_addresses: Vec<WalletAddress> =
                    self.addresses_from_output_pubkeys(&tx_output);

                let quote_info = quotes
                    .iter()
                    .filter(|quote| quote.inner.input == Coin::BTC)
                    // loop through all the btc output addresses for this transaction
                    // to see if it matches a quote
                    .find(|quote_info| {
                        let quote = &quote_info.inner;
                        btc_addresses
                            .iter()
                            .find(|address| address.0 == quote.input_address.0)
                            .is_some()
                    });

                if quote_info.is_none() {
                    continue;
                }

                let quote = &quote_info.unwrap().inner;

                // we only need the amount sent to chainflip address, we don't care about the change
                let mut sent_amount: Option<u128> = None;
                for txout in tx_output {
                    let sent_to = match Address::from_script(
                        &txout.script_pubkey,
                        self.client.get_network_type(),
                    ) {
                        Some(addr) => addr.to_string(),
                        None => continue,
                    };
                    if sent_to == quote.input_address.0 {
                        sent_amount = Some(txout.value as u128);
                    }
                }
                if sent_amount.is_none() || sent_amount.unwrap_or(0) <= 0 {
                    error!("Bitcoin transaction amount must be set and greater than 0. Tx {} in block {}", tx_index, self.next_bitcoin_block);
                    continue;
                }

                let tx = WitnessTx {
                    id: Uuid::new_v4(),
                    quote_id: quote.id,
                    transaction_id: tx.txid().to_string(),
                    transaction_block_number: self.next_bitcoin_block,
                    transaction_index: tx_index as u64,
                    amount: sent_amount.unwrap(),
                    timestamp: Timestamp::now(),
                    coin: Coin::BTC,
                    sender: None,
                };

                witness_txs.push(tx);
            }

            // TODO: Put below code into a util function for sharing between
            // ETH and BTC
            if witness_txs.len() > 0 {
                let side_chain_txs = witness_txs
                    .into_iter()
                    .map(SideChainTx::WitnessTx)
                    .collect();

                provider
                    .add_transactions(side_chain_txs)
                    .expect("Could not publish witness txs");
            }

            self.next_bitcoin_block += 1;
            self.store
                .lock()
                .unwrap()
                .set_data(NEXT_BTC_BLOCK_KEY, Some(self.next_bitcoin_block))
                .expect("Failed to store next bitcoin block");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::btc::TestBitcoinClient;
    use crate::{
        common::WalletAddress,
        side_chain::MemorySideChain,
        utils::test_utils::{
            create_fake_quote_tx_coin_to_loki, get_transactions_provider, store::MemoryKVS,
        },
        vault::transactions::MemoryTransactionsProvider,
    };
    use bitcoin::blockdata::opcodes::all::*;
    use bitcoin::blockdata::opcodes::*;
    use bitcoin::blockdata::script::*;
    use bitcoin::hash_types::Txid;
    use core::str::FromStr;

    type TestTransactionsProvider = MemoryTransactionsProvider<MemorySideChain>;
    struct TestObjects {
        client: Arc<TestBitcoinClient>,
        provider: Arc<Mutex<TestTransactionsProvider>>,
        store: Arc<Mutex<MemoryKVS>>,
        witness: BitcoinWitness<TestTransactionsProvider, TestBitcoinClient, MemoryKVS>,
    }

    // btc output address of 10th transaction on block
    // https://live.blockcypher.com/btc-testnet/block/00000000000000b4e5c133075b925face5b22dccb53112e4c7bf95313e0cf7f2/
    // the base58 public key
    const TENTH_TX_OUTPUT_PK: &str = "msjFLavJYLoF3hs3rgTrmBanpaHntjDgWQ";
    // the hash160 of the base58 public key (how it appears on the chain)
    const TENTH_TX_OUTPUT_PKH: &str = "85f4ba6fc55871cf82fb74519f6551007c7a6ee6";

    /// Generate a script pubkey from a public key (hash160)
    fn script_p2pkh_from_pkh(address: &str) -> Script {
        Builder::new()
            .push_opcode(OP_DUP)
            .push_opcode(OP_HASH160)
            .push_slice(&hex::decode(address).unwrap())
            .push_opcode(OP_EQUALVERIFY)
            .push_opcode(OP_CHECKSIG)
            .into_script()
    }

    fn script_p2sh_from_pkh(input: &str) -> Script {
        Builder::new()
            .push_opcode(OP_FALSE)
            .push_slice(&hex::decode(input).unwrap())
            .into_script()
    }

    // These transactions are 10th and 11th from this block on testnet:
    // https://live.blockcypher.com/btc-testnet/block/00000000000000b4e5c133075b925face5b22dccb53112e4c7bf95313e0cf7f2/
    fn build_tenth_tx() -> Transaction {
        let version = 2;
        let lock_time = 1834584;

        // input
        let previous_output = OutPoint {
            txid: Txid::from_str(
                "ab3e05dce9a3e689dc234014b41daa9c042a4eeb5ee84bcf3d1e674f9a7947b7",
            )
            .unwrap(),
            vout: 1,
        };

        let script_sig = Builder::new()
            .push_opcode(OP_PUSHBYTES_71)
            .push_slice(&hex::decode("3044022049412c3e188f788c4e5842137b6a0966cc03b9e848d7f9275cc78c764cc4b51b022030d6f94fe5176a2ac4e1250e23895e07ac300c95b88efddf87cbece7803899ef01").unwrap())
            .push_opcode(OP_PUSHBYTES_33)
            .push_slice(&hex::decode("039b7651a2e0ac88e4019a813193c7a06d3fb96097d1790fd481dc0892c6a1165e").unwrap())
            .into_script();

        let sequence = 4294967293;
        let witness: Vec<Vec<u8>> = vec![];

        let txin = TxIn {
            previous_output,
            script_sig,
            sequence,
            witness,
        };

        // output
        let script_pubkey1 = script_p2pkh_from_pkh(TENTH_TX_OUTPUT_PKH);

        let txout1 = TxOut {
            value: 86265,
            script_pubkey: script_pubkey1,
        };

        let script_pubkey2 = script_p2pkh_from_pkh("3508c739bed2ccbab856a5fa2b79b3308ab66e41");
        let txout2 = TxOut {
            value: 468175284,
            script_pubkey: script_pubkey2,
        };

        Transaction {
            version,
            lock_time,
            input: vec![txin],
            output: vec![txout1, txout2],
        }
    }

    // invalid output address (based on above 10th tx)
    fn build_invalid_tx() -> Transaction {
        let version = 2;
        let lock_time = 1834584;

        // input
        let previous_output = OutPoint {
            txid: Txid::from_str(
                "ab3e05dce9a3e689dc234014b41daa9c042a4eeb5ee84bcf3d1e674f9a7947b7",
            )
            .unwrap(),
            vout: 1,
        };

        let script_sig = Builder::new()
            .push_opcode(OP_PUSHBYTES_71)
            .push_slice(&hex::decode("3044022049412c3e188f788c4e5842137b6a0966cc03b9e848d7f9275cc78c764cc4b51b022030d6f94fe5176a2ac4e1250e23895e07ac300c95b88efddf87cbece7803899ef01").unwrap())
            .push_opcode(OP_PUSHBYTES_33)
            .push_slice(&hex::decode("039b7651a2e0ac88e4019a813193c7a06d3fb96097d1790fd481dc0892c6a1165e").unwrap())
            .into_script();

        let sequence = 4294967293;
        let witness: Vec<Vec<u8>> = vec![];

        let txin = TxIn {
            previous_output,
            script_sig,
            sequence,
            witness,
        };

        // output
        let script_pubkey1 = script_p2pkh_from_pkh(TENTH_TX_OUTPUT_PKH);

        let txout1 = TxOut {
            value: 86265,
            script_pubkey: script_pubkey1,
        };

        // invalid scriptPubKey
        let script_pubkey2 =
            script_p2pkh_from_pkh("3508c739bed2ccbab856a5fa2b79b3308ab66e41aaaaaa");
        let txout2 = TxOut {
            value: 468175284,
            script_pubkey: script_pubkey2,
        };

        Transaction {
            version,
            lock_time,
            input: vec![txin],
            output: vec![txout1, txout2],
        }
    }

    fn build_eleventh_tx() -> Transaction {
        // txin1
        let previous_output1 = OutPoint {
            txid: Txid::from_str(
                "a43d1728807fd5e4e08acc543add6770ad0e1816ca8982b0e6ca1f8545b220e1",
            )
            .unwrap(),
            vout: 0,
        };

        let witness1: Vec<Vec<u8>> = vec![
            vec![
                48, 68, 2, 32, 110, 40, 173, 175, 213, 122, 208, 247, 95, 255, 82, 244, 189, 212,
                183, 107, 159, 78, 182, 205, 55, 155, 59, 1, 122, 177, 32, 152, 36, 37, 83, 182, 2,
                32, 113, 170, 82, 236, 242, 16, 163, 76, 184, 214, 81, 105, 142, 7, 149, 48, 72,
                153, 80, 138, 223, 12, 40, 166, 13, 125, 43, 197, 129, 31, 30, 26, 1,
            ],
            vec![
                3, 138, 81, 199, 219, 90, 183, 55, 126, 141, 95, 154, 6, 190, 107, 107, 70, 201, 0,
                223, 81, 175, 148, 149, 167, 159, 62, 240, 26, 70, 199, 31, 73,
            ],
        ];
        let script_sig1 = Builder::new()
            .push_opcode(OP_PUSHBYTES_22)
            .push_slice(&hex::decode("0014b5d5a81ba37d47ad5fe276b23f3973f56b70a459").unwrap())
            .into_script();

        let txin1 = TxIn {
            previous_output: previous_output1,
            script_sig: script_sig1,
            sequence: 4294967294,
            witness: witness1,
        };

        // txin2
        let previous_output2 = OutPoint {
            txid: Txid::from_str(
                "e7f5afa3cf5dd41a8856a89ac9385a2556b320090396d40ab92be1807dabed40",
            )
            .unwrap(),
            vout: 1,
        };

        let witness2: Vec<Vec<u8>> = vec![
            vec![
                48, 68, 2, 32, 97, 113, 134, 89, 136, 183, 98, 109, 178, 199, 130, 83, 46, 237,
                133, 241, 68, 162, 105, 247, 231, 229, 89, 23, 110, 11, 152, 100, 110, 83, 54, 241,
                2, 32, 88, 248, 33, 160, 222, 210, 93, 64, 225, 12, 221, 90, 16, 101, 164, 11, 101,
                239, 247, 59, 15, 8, 111, 124, 188, 154, 128, 218, 66, 23, 115, 95, 1,
            ],
            vec![
                2, 194, 138, 203, 55, 251, 136, 123, 77, 225, 183, 156, 42, 17, 148, 75, 131, 254,
                237, 127, 17, 12, 39, 86, 60, 183, 71, 93, 12, 249, 112, 14, 207,
            ],
        ];

        let txin2 = TxIn {
            previous_output: previous_output2,
            script_sig: Script::new(),
            sequence: 4294967294,
            witness: witness2,
        };

        let input = vec![txin1, txin2];

        // output
        let script_pubkey1 = script_p2sh_from_pkh("b98c5391fda89c3cecc062244dcad7359bc1628d");

        let script_pubkey2 = script_p2sh_from_pkh("7fe664e92208a060b1586f593c82f99b866a0d44");

        let output = vec![
            TxOut {
                value: 150000,
                script_pubkey: script_pubkey1,
            },
            TxOut {
                value: 1004374,
                script_pubkey: script_pubkey2,
            },
        ];

        Transaction {
            version: 2,
            lock_time: 1834584,
            input,
            output,
        }
    }

    fn setup() -> TestObjects {
        let client = Arc::new(TestBitcoinClient::new());
        let provider = Arc::new(Mutex::new(get_transactions_provider()));
        let store = Arc::new(Mutex::new(MemoryKVS::new()));
        let witness = BitcoinWitness::new(client.clone(), provider.clone(), store.clone());

        TestObjects {
            client,
            provider,
            store,
            witness,
        }
    }

    #[tokio::test]
    async fn poll_next_main_chain_test() {
        let params = setup();
        let client = params.client;
        let provider = params.provider;
        let mut witness = params.witness;

        let tx10 = build_tenth_tx();
        let tx11 = build_eleventh_tx();
        client.add_block(vec![tx10, tx11]);

        // uses a BTC address
        let eth_quote = create_fake_quote_tx_coin_to_loki(
            Coin::ETH,
            WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5"),
        );
        // this quote will be witnessed
        let btc_quote = create_fake_quote_tx_coin_to_loki(
            Coin::BTC,
            WalletAddress(TENTH_TX_OUTPUT_PK.to_string()),
        );

        {
            let mut provider = provider.lock().unwrap();
            provider
                .add_transactions(vec![eth_quote.clone().into(), btc_quote.clone().into()])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 2);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        // Poll and add a witness tx
        witness.poll_next_main_chain().await;

        let provider = provider.lock().unwrap();

        assert_eq!(provider.get_quote_txs().len(), 2);
        assert_eq!(
            provider.get_witness_txs().len(),
            1,
            "Expected a witness transaction to be added"
        );

        let witness_tx = &provider
            .get_witness_txs()
            .first()
            .expect("Expected a witness transaction to exist")
            .inner;

        // 10th transaction details should be what is in the witness tx
        assert_eq!(witness_tx.quote_id, btc_quote.id);
        assert_eq!(
            witness_tx.transaction_id,
            build_tenth_tx().txid().to_string()
        );
        // NB: The test transactions are actually from block 1,834,585 on the testnet
        // here we are simulating retrieving from our starting block
        assert_eq!(witness_tx.transaction_block_number, START_BLOCK);
        assert_eq!(witness_tx.amount, 86265);
        assert_eq!(witness_tx.sender, None);
    }

    #[tokio::test]
    async fn add_witness_saves_next_bitcoin_block() {
        let params = setup();
        let client = params.client;
        let store = params.store;
        let mut witness = params.witness;

        let tx10 = build_tenth_tx();
        let tx11 = build_eleventh_tx();
        client.add_block(vec![tx10, tx11]);

        // Pre-conditions
        assert_eq!(witness.next_bitcoin_block, START_BLOCK);
        assert!(store
            .lock()
            .unwrap()
            .get_data::<u64>(NEXT_BTC_BLOCK_KEY)
            .is_none());

        witness.poll_next_main_chain().await;

        let next_block_key = store.lock().unwrap().get_data::<u64>(NEXT_BTC_BLOCK_KEY);
        assert_eq!(next_block_key, Some(witness.next_bitcoin_block));
        assert_ne!(next_block_key, Some(START_BLOCK));
    }

    #[tokio::test]
    async fn when_1_invalid_txout_continue() {
        let params = setup();
        let client = params.client;
        let provider = params.provider;
        let mut witness = params.witness;

        let tx10 = build_invalid_tx();
        let tx11 = build_eleventh_tx();
        client.add_block(vec![tx10, tx11]);

        // Add a quote so we can witness it
        let eth_quote = create_fake_quote_tx_coin_to_loki(
            Coin::ETH,
            WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5"),
        );
        let btc_quote = create_fake_quote_tx_coin_to_loki(
            Coin::BTC,
            WalletAddress(TENTH_TX_OUTPUT_PK.to_string()),
        );

        {
            let mut provider = provider.lock().unwrap();
            provider
                .add_transactions(vec![eth_quote.clone().into(), btc_quote.clone().into()])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 2);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        // Poll and add a witness tx
        witness.poll_next_main_chain().await;

        let provider = provider.lock().unwrap();

        assert_eq!(provider.get_quote_txs().len(), 2);
        assert_eq!(
            provider.get_witness_txs().len(),
            1,
            "Expected a witness transaction to be added"
        );

        let witness_tx = &provider
            .get_witness_txs()
            .first()
            .expect("Expected a witness transaction to exist")
            .inner;

        assert_eq!(witness_tx.quote_id, btc_quote.id);
        // NB: The test transactions are actually from block 1,834,585 on the testnet
        // here we are simulating retrieving from our starting block
        assert_eq!(witness_tx.transaction_block_number, START_BLOCK);
        assert_eq!(witness_tx.amount, 86265);
        assert_eq!(witness_tx.sender, None);
    }
}
