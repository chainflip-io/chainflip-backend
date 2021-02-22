use super::{BitcoinSPVClient, IBitcoinSend, SendTransaction};
use crate::{
    common::{GenericCoinAmount, WalletAddress},
    utils::bip44::KeyPair,
};
use async_trait::async_trait;
use bitcoin::{
    blockdata::{script::*, transaction::*},
    consensus::encode::serialize,
    util::bip143::SigHashCache,
    Transaction, Txid,
};
use chainflip_common::types::coin::Coin;
use core::str::FromStr;
use hdwallet::secp256k1::{Message, PublicKey, Secp256k1};
use serde::Deserialize;
use std::fmt::{Display, Formatter};

/// Electrum SPV error
#[derive(Debug, Deserialize)]
pub struct BtcSPVError {
    code: i32,
    message: String,
}

/// Electrum spv response
#[derive(Debug, Deserialize)]
pub struct BtcSPVResponse {
    error: Option<BtcSPVError>,
    result: Option<serde_json::Value>,
}

/// Method for calculating fee

/// Mempool and ETA depend on market conditions
#[derive(Debug)]
pub enum FeeMethod {
    /// Static is returns a constant value, does not take market conditions into account. (0, 3000] sats/byte
    /// These are stored as levels (0, 4], e.g. level 0 is 1000sats/kVbyte (1 sat/byte, the lowest possible)
    Static,
    /// ETA is based on number of blocks to confirm at tx, on average a BTC block takes ~10mins
    Eta,
    /// Mempool bases it off mempool congestion
    Mempool,
}

impl FromStr for FeeMethod {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "eta" => Ok(FeeMethod::Eta),
            "static" => Ok(FeeMethod::Static),
            "mempool" => Ok(FeeMethod::Mempool),
            _ => Err(()),
        }
    }
}

impl Display for FeeMethod {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            FeeMethod::Eta => write!(f, "eta"),
            FeeMethod::Static => write!(f, "static"),
            FeeMethod::Mempool => write!(f, "mempool"),
        }
    }
}

/// Can fetch UTXOs of particular addresses and send from a linked wallet using
/// bitcoin's Simple Payment Verification (SPV)
///
/// ## Setup
/// To setup an SPV client from python source
/// 1. Download and install Electrum from python source https://electrum.org/#download
/// 2. Start the daemon ./run_electrum daemon (--testnet)
/// 3. Set the config as desired
/// ```ignore
/// ./run_electrum setconfig rpcport 7777 (--testnet)
/// ./run_electrum setconfig rpcuser bitcoinrpc (--testnet)
/// ./run_electrum setconfig rpcpassword Password123 (--testnet)
/// ```
/// 4. To send funds you must load in a wallet that contains funds
/// `./run_electrum load_wallet (--testnet)` will load the wallet file `default_wallet` by default
/// This will load from your `ELECTRUM_HOME` directory, normally something like `/Users/user/.electrum`
/// 5. On initialising the client, use the above configuration settings, e.g.
/// `BtcSPVClient::new(7777, "bitcoinrpc".to_string(), "Password123".to_string())`
#[derive(Clone)]
pub struct BtcSPVClient {
    port: u16,
    username: String,
    password: String,
    network: bitcoin::Network,
    change_address: bitcoin::Address,
}

/// Contains the UTXOs to be used in a transaction, and the amount of change returned to the sender
pub struct SelectInputsResponse {
    utxos: Vec<BtcUTXO>,
    change: u128,
}

/// Struct to contain response of getaddressbalance SPV RPC call
#[derive(Debug, Deserialize)]
pub struct GetBalanceResponse {
    confirmed: String,
    unconfirmed: String,
}

/// Use greedy selection algorithm to select the utxos for a transaction
fn select_inputs_greedy(target: u128, utxos: Vec<BtcUTXO>) -> Option<SelectInputsResponse> {
    if utxos.len() == 0 {
        return None;
    }

    let (mut lessers, greaters): (Vec<BtcUTXO>, Vec<BtcUTXO>) =
        utxos.into_iter().partition(|utxo| {
            let value = utxo.value as u128;
            value < target
        });

    let mut input_utxos: Vec<BtcUTXO> = vec![];

    let mut change: u128 = 0;
    let mut can_send: bool = false;
    if greaters.len() > 0 {
        let min_greater = greaters
            .into_iter()
            .min_by(|x, y| x.value.cmp(&y.value))
            .unwrap();
        change = min_greater.value as u128 - target;
        input_utxos.push(min_greater.to_owned());
        can_send = true;
    } else {
        lessers.sort_by(|x, y| x.value.cmp(&y.value));
        let mut sum: u64 = 0;
        for utxo in lessers {
            sum = sum
                .checked_add(utxo.value)
                .expect("Sum overflowed when aggregating UTXOs");
            input_utxos.push(utxo);

            // we have enough utxos to build the tx
            if sum as u128 >= target {
                change = sum as u128 - target;
                can_send = true;
                break;
            }
        }
    }

    // if it doesn't make up the full amount
    if !can_send {
        return None;
    }

    Some(SelectInputsResponse {
        utxos: input_utxos,
        change,
    })
}

fn wallet_address_from_pubkey(
    pubkey: PublicKey,
    network: bitcoin::Network,
) -> Result<WalletAddress, String> {
    let pubkey = bitcoin::PublicKey {
        // convert between two crate versions of PublicKey
        key: bitcoin::secp256k1::PublicKey::from_slice(&pubkey.serialize()).unwrap(),
        // for Segwit the key must be compressed
        compressed: true,
    };
    let p2pwkh_addr = bitcoin::Address::p2wpkh(&pubkey, network).map_err(|e| e.to_string())?;

    let wallet_address = WalletAddress(p2pwkh_addr.to_string());
    Ok(wallet_address)
}

impl BtcSPVClient {
    /// Create new BtcSPVClient
    pub fn new(
        port: u16,
        username: String,
        password: String,
        network: bitcoin::Network,
        change_address: bitcoin::Address,
    ) -> Self {
        BtcSPVClient {
            port,
            username,
            password,
            network,
            change_address,
        }
    }

    async fn send_req_inner(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let client = reqwest::Client::new();

        let url = format!(
            "http://{}:{}@localhost:{}",
            self.username, self.password, self.port
        );

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "curltext",
            "method": method,
            "params": params,
        });

        let res = client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let text = res.text().await.map_err(|err| err.to_string())?;

        let res: BtcSPVResponse = serde_json::from_str(&text).map_err(|err| err.to_string())?;

        if let Some(err) = res.error {
            error!("Bitcoin SPV RPC error");
            return Err(err.message.to_owned());
        }

        if let Some(result) = res.result {
            Ok(result)
        } else {
            Err("Neither result no error present in response".to_owned())
        }
    }

    // Get transaction fee in satoshis, based on number of UTXOs in the transaction
    async fn get_fee_from_num_utxos(
        &self,
        num_utxos_in_tx: u64,
        fee_method: FeeMethod,
        // fee_level is dependent on fee_method
        fee_level: u32,
    ) -> Result<u64, String> {
        // 11 bytes is the base segwit tx size
        const BASE_SEGWIT_TX_SIZE: u64 = 11;
        const SEGWIT_INPUT_SIZE: u64 = 68;
        const P2WPKH_OUTPUT_SIZE: u64 = 32;
        // this is known, we have a "send to" and a "change" output
        const NUM_OUTPUTS: u64 = 2;

        let v_size = BASE_SEGWIT_TX_SIZE
            + SEGWIT_INPUT_SIZE * num_utxos_in_tx
            + P2WPKH_OUTPUT_SIZE * NUM_OUTPUTS;
        let v_size = v_size as u64;

        let fee_rate = self.get_fee_rate(fee_method, fee_level).await?;

        // get_fee_rate returns in sats/kvBytes (virtual kilobytes)
        let fee_rate = fee_rate as f64 / 1000 as f64;
        let mut fee_rate = fee_rate.ceil() as u64;

        // the lowest the fee per byte can be is 1 sat
        if fee_rate < 1 {
            fee_rate = 1;
        }

        let fee = v_size
            .checked_mul(fee_rate)
            .expect("Virtual tx size * fee rate overflowed");

        Ok(fee)
    }

    /// Constructs a raw transaction, aggregating a wallet's UTXOs for sending
    async fn construct_raw_tx(
        &self,
        send_transaction: &SendTransaction,
    ) -> Result<(Transaction, Vec<BtcUTXO>), String> {
        let amount = send_transaction.amount;
        let sender_keypair = send_transaction.from.clone();

        let sender_wallet_address =
            wallet_address_from_pubkey(sender_keypair.public_key, self.network)?;

        let utxos = self.get_address_unspent(&sender_wallet_address).await?.0;

        let select_inputs = match select_inputs_greedy(amount.to_atomic(), utxos) {
            Some(inputs) => inputs,
            None => {
                return Err(format!(
                    "Cannot send to {}, this wallet has less than {} Satoshis",
                    send_transaction.to.to_string(),
                    amount.to_atomic()
                ));
            }
        };
        let inputs: Vec<BtcUTXO> = select_inputs.utxos;
        let fee = self
            .get_fee_from_num_utxos(inputs.len() as u64, FeeMethod::Eta, 1)
            .await?;

        if fee as u128 > amount.to_atomic() {
            return Err(format!(
                "Fee of {} is greater than send amount of {} (in atomic units)",
                fee,
                amount.to_atomic()
            ));
        }

        let mut txins = vec![];
        for input in &inputs {
            let outpoint = OutPoint {
                txid: Txid::from_str(&input.tx_hash).map_err(|e| e.to_string())?,
                vout: input.tx_pos as u32,
            };

            let txin = TxIn {
                previous_output: outpoint,
                // empty sig for segwit UTXOs
                script_sig: Script::new(),
                sequence: 0xFFFFFFFF,
                // added by signing step
                witness: Vec::default(),
            };
            txins.push(txin);
        }

        let send_to_script_pubkey = send_transaction.to.script_pubkey();

        // "Actual" outgoing transaction
        let send_tx_out = TxOut {
            value: amount.to_atomic() as u64 - fee, // the user, not chainflip, pays the fee
            script_pubkey: send_to_script_pubkey,
        };

        let mut txouts: Vec<TxOut> = vec![send_tx_out];

        // Change (unspent from inputs must be sent back to sending wallet)
        if select_inputs.change > 0 {
            let change_tx_out = TxOut {
                value: select_inputs.change as u64,
                script_pubkey: self.change_address.script_pubkey(),
            };
            txouts.push(change_tx_out);
        }

        let transaction = Transaction {
            version: 2,
            // confirm as soon as possible
            lock_time: 0,
            input: txins,
            output: txouts,
        };

        Ok((transaction, inputs))
    }

    // takes a mutable tx and adds the required signatures for a segwit transaction
    async fn sign_tx(
        &self,
        mut tx: Transaction,
        utxos: Vec<BtcUTXO>,
        keypair: &KeyPair,
    ) -> Result<Transaction, String> {
        let btc_sender_pubkey = bitcoin::PublicKey {
            key: bitcoin::secp256k1::PublicKey::from_slice(&keypair.public_key.serialize())
                .unwrap(),
            compressed: true,
        };

        let sender_script_pubkey =
            bitcoin::Address::p2pkh(&btc_sender_pubkey, self.network).script_pubkey();

        // We must sign each input individually
        let input_count = tx.input.len();
        let mut sig_hasher = SigHashCache::new(&mut tx);
        for index in 0..input_count {
            let sighash = sig_hasher
                .signature_hash(
                    index,
                    &sender_script_pubkey,
                    utxos[index].value,
                    SigHashType::All,
                )
                .as_hash();

            // sign the sighash
            let secp = Secp256k1::new();
            let msg = Message::from_slice(&sighash[..]).map_err(|err| err.to_string())?;
            let sig = secp.sign(&msg, &keypair.private_key);

            match secp.verify(&msg, &sig, &keypair.public_key) {
                Ok(res) => res,
                Err(err) => {
                    error!("Signing of sighash failed with error: {}", err);
                    return Err(err.to_string());
                }
            };
            // Add byte string
            let mut sig = sig.serialize_der().to_vec();
            // Add SIGHASHALL byte
            sig.push(0x01);

            let pubkey_bytes = btc_sender_pubkey.key.serialize().to_vec();

            sig_hasher.access_witness(index).push(sig);
            sig_hasher.access_witness(index).push(pubkey_bytes);
        }

        Ok(tx)
    }

    // take a pre-prepared serialized tx and broadcast it to the network
    async fn broadcast_tx(&self, tx: String) -> Result<Txid, String> {
        // only takes one "tx" arg
        let params = serde_json::json!({ "tx": tx });

        let res = self
            .send_req_inner("broadcast", params)
            .await
            .map_err(|err| err.to_string())?;

        let txid_str = res
            .as_str()
            .ok_or("Could not cast result to string")
            .map_err(|e| e.to_string())?;

        let txid = Txid::from_str(txid_str).map_err(|e| e.to_string())?;

        Ok(txid)
    }

    async fn get_fee_rate(&self, fee_method: FeeMethod, fee_level: u32) -> Result<u64, String> {
        let fee_method_string = fee_method.to_string();
        let params = serde_json::json!(
        {
            "fee_method": fee_method_string,
            "fee_level": fee_level
        });
        let res = self
            .send_req_inner("getfeerate", params)
            .await
            .map_err(|err| err.to_string())?;

        let feerate = res
            .as_u64()
            .ok_or("Could not cast result to u64")
            .map_err(|e| e.to_string())?;

        Ok(feerate)
    }
}

#[derive(Debug, Clone, Deserialize)]
/// Contains the details of an Unspent transaction output
pub struct BtcUTXO {
    /// The block height of the UTXO
    pub height: u64,
    /// The transaction hash
    pub tx_hash: String,
    /// The index of the transaction in the block
    pub tx_pos: u64,
    /// The amount of the UTXO
    pub value: u64,
}

impl BtcUTXO {
    /// Create a new UTXO
    pub fn new(height: u64, tx_hash: String, tx_pos: u64, value: u64) -> Self {
        BtcUTXO {
            height,
            tx_hash,
            tx_pos,
            value,
        }
    }
}

#[derive(Debug, Deserialize)]
/// Wrapper struct for an AddressUnspentResponse
pub struct AddressUnspentResponse(pub Vec<BtcUTXO>);

#[async_trait]
impl BitcoinSPVClient for BtcSPVClient {
    /// Returns UTXO list of any address
    async fn get_address_unspent(
        &self,
        address: &WalletAddress,
    ) -> Result<AddressUnspentResponse, String> {
        let params = serde_json::json!({ "address": address });

        let res = self
            .send_req_inner("getaddressunspent", params)
            .await
            .map_err(|err| err.to_string())?;

        let unspent_response: AddressUnspentResponse =
            serde_json::from_value(res).map_err(|err| err.to_string())?;

        Ok(unspent_response)
    }

    async fn get_estimated_fee(
        &self,
        send_tx: &SendTransaction,
        fee_method: FeeMethod,
        fee_level: u32,
    ) -> Result<u64, String> {
        let wallet_address = wallet_address_from_pubkey(send_tx.from.public_key, self.network)?;

        let utxos = self.get_address_unspent(&wallet_address).await?.0;
        let inputs = select_inputs_greedy(send_tx.amount.to_atomic(), utxos).ok_or(format!(
            "Wallet {} does not have {} Sats to make this transaction",
            wallet_address.to_string(),
            send_tx.amount.to_atomic()
        ))?;

        self.get_fee_from_num_utxos(inputs.utxos.len() as u64, fee_method, fee_level)
            .await
    }
}

#[async_trait]
impl IBitcoinSend for BtcSPVClient {
    /// Sends a transaction to an address.
    async fn send(&self, send_tx: &SendTransaction) -> Result<Txid, String> {
        let (tx, utxos) = self.construct_raw_tx(&send_tx).await?;

        let signed_tx = self.sign_tx(tx, utxos, &send_tx.from).await?;

        let signed_hex_tx = hex::encode(serialize(&signed_tx));

        self.broadcast_tx(signed_hex_tx).await
    }

    /// Get the confirmed balance of any BTC address
    async fn get_address_balance(
        &self,
        address: WalletAddress,
    ) -> Result<GenericCoinAmount, String> {
        let params = serde_json::json!({ "address": address });

        let res = self
            .send_req_inner("getaddressbalance", params)
            .await
            .map_err(|err| err.to_string())?;

        let get_balance_response: GetBalanceResponse =
            serde_json::from_value(res).map_err(|err| err.to_string())?;

        let generic_amt =
            GenericCoinAmount::from_decimal_string(Coin::BTC, &get_balance_response.confirmed[..]);
        Ok(generic_amt)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::consensus::deserialize;
    use hex::FromHex;

    // The (testnet) segwit public key for this address is: tb1q6898gg3tkkjurdpl4cghaqgmyvs29p4x4h0552
    fn get_key_pair() -> KeyPair {
        KeyPair::from_private_key(
            "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f",
        )
        .unwrap()
    }

    fn get_test_btc_spv_client() -> BtcSPVClient {
        BtcSPVClient::new(
            7777,
            "bitcoinrpc".to_string(),
            "Password123".to_string(),
            bitcoin::Network::Testnet,
            // change address is same as that derived from the key pair above
            bitcoin::Address::from_str("tb1q6898gg3tkkjurdpl4cghaqgmyvs29p4x4h0552").unwrap(),
        )
    }

    fn decode_hex_tx(hex_tx: &str) -> Result<Transaction, String> {
        let tx_bytes = Vec::from_hex(hex_tx).map_err(|err| err.to_string())?;

        let transaction: Result<Transaction, String> =
            deserialize(&tx_bytes).map_err(|err| err.to_string());

        transaction
    }

    #[test]
    fn decode_tx_test() {
        let hex = "020000000001025a54a3e8f52e70152c1d24d6fa6a57b6ffdf9565821e5c26468aceea14677a560100000000fdffffff5b9424587e602295d16a02c338574517d0ab0f7a6f15085964402f9fb63dfccf0000000000fdffffff02e803000000000000160014d282440c27ea7cc7a04d913139831b2c0970b51a903d0f0000000000160014bd058f3dcb7964e4b6ac16528bbd45dd6e5f74b10247304402200f37cdc12037dc1591712b5d3253fded2859dc10dce192bbaf4ec8a6727b063902205eec770c6729dc54df40aadf736c36112a7e97c8733d987f202046402fd461e4012102fafc20310f52e9152f1e81ba76329d81211d54e4e1473dd4c70ab031e6cb5a2702473044022078dee3a8b6218821130188b160bc8fe2876c88b97645d1e85542545b8020c27b022008d2a0f863e2583afde6397dfae2ec4b8fe61191bc74143f7686028674d45735012102fafc20310f52e9152f1e81ba76329d81211d54e4e1473dd4c70ab031e6cb5a2750661c00";
        let tx = decode_hex_tx(hex);
        assert!(tx.is_ok());
        let real_tx = tx.unwrap();
        let outputs = real_tx.output;
        assert_eq!(outputs.first().unwrap().value, 1000);
    }

    #[tokio::test]
    #[ignore = "Dependent on BTC SPV Client"]
    async fn get_address_unspent_test() {
        let client = get_test_btc_spv_client();
        let address = WalletAddress("tb1q62pygrp8af7v0gzdjycnnqcm9syhpdg6a0kunk".to_string());
        let result = client.get_address_unspent(&address).await;

        assert!(result.is_ok());
    }

    fn fake_return_utxos() -> Vec<BtcUTXO> {
        let utxo1 = BtcUTXO::new(
            250000,
            "a9ec47601a25f0cc27c63c78cab3d446294c5eccb171f3973ee9979c00bee432".to_string(),
            0,
            2000,
        );
        let utxo2 = BtcUTXO::new(
            250002,
            "b9ec47601a25f0cd27c63c78cab3d446294c5eccb171f3973ee9979c00bee442".to_string(),
            0,
            4000,
        );
        vec![utxo1, utxo2]
    }

    #[tokio::test]
    #[ignore = "Depends on SPV client"]
    async fn constructs_raw_tx_signs_and_sends_tx() {
        let client = get_test_btc_spv_client();

        let amount = GenericCoinAmount::from_atomic(Coin::BTC, 300);
        let keypair = get_key_pair();

        let send_to_btc_pubkey = bitcoin::PublicKey {
            // convert between two crate versions of PublicKey
            key: bitcoin::secp256k1::PublicKey::from_slice(&keypair.public_key.serialize())
                .unwrap(),
            // Segwit is always compressed
            compressed: true,
        };

        let send_to_btc_addr =
            bitcoin::Address::p2wpkh(&send_to_btc_pubkey, client.network).unwrap();

        // send to self
        let send_transaction = SendTransaction {
            from: keypair,
            to: send_to_btc_addr,
            amount,
        };

        let tx = client.send(&send_transaction).await;
        assert!(tx.is_ok());
    }

    // if we attempt to send 200 sats but estimated fee is 250, what should happen?
    #[tokio::test]
    #[ignore = "Depends on SPV client"]
    async fn fee_greater_than_amount_to_be_sent() {
        let client = get_test_btc_spv_client();

        // fee will always be more than 10 sats
        let amount = GenericCoinAmount::from_atomic(Coin::BTC, 10);
        let keypair = get_key_pair();

        let send_to_btc_pubkey = bitcoin::PublicKey {
            // convert between two crate versions of PublicKey
            key: bitcoin::secp256k1::PublicKey::from_slice(&keypair.public_key.serialize())
                .unwrap(),
            // Segwit is always compressed
            compressed: true,
        };

        let send_to_btc_addr =
            bitcoin::Address::p2wpkh(&send_to_btc_pubkey, client.network).unwrap();

        println!("Send to this addr: {}", send_to_btc_addr);

        // send to self
        let send_transaction = SendTransaction {
            from: keypair,
            to: send_to_btc_addr,
            amount,
        };

        let tx = client.send(&send_transaction).await;

        assert!(tx.is_err());

        let err = tx.unwrap_err();

        assert!(err.contains("is greater than send amount of"));
    }

    #[tokio::test]
    #[ignore = "Depends on SPV Client"]
    async fn get_fee_rate() {
        let client = get_test_btc_spv_client();
        // confirm with an ETA confirmation of 4 blocks
        let resp = client.get_fee_rate(FeeMethod::Eta, 4).await;
        assert!(resp.is_ok());
    }

    #[test]
    fn get_greedy_inputs_greater() {
        let target = 500;
        let utxos = fake_return_utxos();
        let inputs = select_inputs_greedy(target, utxos.clone()).unwrap().utxos;
        let first = inputs.first().unwrap();
        assert_eq!(
            first.tx_hash,
            "a9ec47601a25f0cc27c63c78cab3d446294c5eccb171f3973ee9979c00bee432"
        );
        assert_eq!(inputs.len(), 1);

        let target = 3000;
        let select_inputs = select_inputs_greedy(target, utxos.clone()).unwrap();
        let inputs = select_inputs.utxos;
        let change = select_inputs.change;
        let first = inputs.first().unwrap();
        assert_eq!(
            first.tx_hash,
            "b9ec47601a25f0cd27c63c78cab3d446294c5eccb171f3973ee9979c00bee442"
        );
        assert_eq!(inputs.len(), 1);
        assert_eq!(change, 1000);
    }

    #[test]
    fn get_greedy_inputs_combined() {
        let target = 5000;
        let utxos = fake_return_utxos();
        let select_inputs = select_inputs_greedy(target, utxos.clone()).unwrap();
        let inputs = select_inputs.utxos;
        let change = select_inputs.change;
        // both utxos required to make up the full amount
        assert_eq!(inputs.len(), 2);
        assert_eq!(change, 1000);
    }

    #[tokio::test]
    #[ignore = "Depends on SPV Client"]
    async fn get_estimated_fee() {
        let client = get_test_btc_spv_client();
        let amount = GenericCoinAmount::from_atomic(Coin::BTC, 400);
        let keypair = get_key_pair();

        let send_to_btc_pubkey = bitcoin::PublicKey {
            // convert between two crate versions of PublicKey
            key: bitcoin::secp256k1::PublicKey::from_slice(&keypair.public_key.serialize())
                .unwrap(),
            // Segwit is always compressed
            compressed: true,
        };

        let send_to_btc_addr =
            bitcoin::Address::p2wpkh(&send_to_btc_pubkey, client.network).unwrap();

        // send to self
        let send_transaction = SendTransaction {
            from: keypair,
            to: send_to_btc_addr,
            amount,
        };

        let fee = client
            .get_estimated_fee(&send_transaction, FeeMethod::Eta, 1)
            .await;
        assert!(fee.is_ok());
        let fee = fee.unwrap();
        println!("The fee returned is: {}", fee);
    }

    #[tokio::test]
    #[ignore = "Depends on SPV Client"]
    async fn get_address_balance_test() {
        let client = get_test_btc_spv_client();
        let address = WalletAddress("tb1q62pygrp8af7v0gzdjycnnqcm9syhpdg6a0kunk".to_string());
        let result = client.get_address_balance(address).await;

        assert!(result.is_ok());
    }

    #[test]
    fn wallet_address_from_pubkey_test() {
        let keypair = get_key_pair();

        let wallet_address =
            wallet_address_from_pubkey(keypair.public_key, bitcoin::Network::Bitcoin);
        assert!(wallet_address.is_ok());
        let wallet_address = wallet_address.unwrap();
        assert_eq!(
            wallet_address,
            WalletAddress("bc1q6898gg3tkkjurdpl4cghaqgmyvs29p4xl3580e".to_string())
        );
    }
}
