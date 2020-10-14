use serde::Deserialize;

use crate::common::{coins::CoinAmount, Coin, GenericCoinAmount, WalletAddress};
use bitcoin::consensus::encode::deserialize;
use bitcoin::Transaction;
use hex::FromHex;

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


/// Can fetch UTXOs of particular addresses and send from a linked wallet using
/// bitcoins Simple Payment Verification (SPV)
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
pub struct BtcSPVClient {
    port: u16,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
/// Contains the details of an Unspent transaction output
pub struct BtcUTXO {
    height: u64,
    tx_hash: String,
    tx_pos: u64,
    value: u64,
}

#[derive(Debug, Deserialize)]
/// Wrapper struct for an AddressUnspentResponse
pub struct AddressUnspentResponse(Vec<BtcUTXO>);

impl BtcSPVClient {
    fn new(port: u16, username: String, password: String) -> Self {
        BtcSPVClient {
            port,
            username,
            password,
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

        debug!(
            "Bitcoin SPV Wallet RPC: /{}. Sending params: {}",
            method,
            params.to_string()
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

    /// Returns UTXO list of any address
    async fn get_address_unspent(
        &self,
        address: WalletAddress,
    ) -> Result<AddressUnspentResponse, String> {
        // let mut params = serde_json::json!({});
        let params = serde_json::json!({ "address": address });
        // params["address"] = address.to_string().into();

        let res = self
            .send_req_inner("getaddressunspent", params)
            .await
            .map_err(|err| err.to_string())?;

        let unspent_response: AddressUnspentResponse =
            serde_json::from_value(res).map_err(|err| err.to_string())?;

        Ok(unspent_response)
    }

    fn decode_hex_tx(&self, hex_tx: &str) -> Result<Transaction, String> {
        let tx_bytes = Vec::from_hex(hex_tx).map_err(|err| err.to_string())?;

        let transaction: Result<Transaction, String> =
            deserialize(&tx_bytes).map_err(|err| err.to_string());

        transaction
    }

    /// Sends a transaction to an address.
    /// # Prerequisite
    /// Wallet must be loaded into the electrum client for the funds to be spent
    async fn send(
        &self,
        destination: WalletAddress,
        atomic_amount: u128,
    ) -> Result<Transaction, String> {
        // Convert atomic amount to BTC amount the rpc expects
        let amount = GenericCoinAmount::from_atomic(Coin::BTC, atomic_amount);
        let btc_amount = amount.to_decimal();

        // amount in BTC
        let params = serde_json::json!({
            "destination": destination,
            "amount": btc_amount
        });

        // returns a hex string of the transaction
        let res = self
            .send_req_inner("payto", params)
            .await
            .map_err(|err| err.to_string())?;

        let hex_tx = res.as_str().ok_or("Could not cast result to string")?;

        let tx = self.decode_hex_tx(hex_tx)?;

        Ok(tx)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn get_test_BtcSPVClient() -> BtcSPVClient {
        BtcSPVClient::new(7777, "bitcoinrpc".to_string(), "Password123".to_string())
    }

    #[tokio::test]
    #[ignore = "Requires local setup and dependent on chain values that may change"]
    async fn get_address_unspent_test() {
        let client = get_test_BtcSPVClient();
        let address = WalletAddress("tb1q62pygrp8af7v0gzdjycnnqcm9syhpdg6a0kunk".to_string());
        let result = client.get_address_unspent(address).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "Requires local setup and dependent on chain values that may change"]
    async fn send_test() {
        let client = get_test_BtcSPVClient();

        let send_to = WalletAddress("tb1q62pygrp8af7v0gzdjycnnqcm9syhpdg6a0kunk".to_string());

        let data = client.send(send_to, 100).await;

        assert!(data.is_ok());
    }

    #[test]
    fn decode_tx_test() {
        let client = get_test_BtcSPVClient();
        let hex = "020000000001025a54a3e8f52e70152c1d24d6fa6a57b6ffdf9565821e5c26468aceea14677a560100000000fdffffff5b9424587e602295d16a02c338574517d0ab0f7a6f15085964402f9fb63dfccf0000000000fdffffff02e803000000000000160014d282440c27ea7cc7a04d913139831b2c0970b51a903d0f0000000000160014bd058f3dcb7964e4b6ac16528bbd45dd6e5f74b10247304402200f37cdc12037dc1591712b5d3253fded2859dc10dce192bbaf4ec8a6727b063902205eec770c6729dc54df40aadf736c36112a7e97c8733d987f202046402fd461e4012102fafc20310f52e9152f1e81ba76329d81211d54e4e1473dd4c70ab031e6cb5a2702473044022078dee3a8b6218821130188b160bc8fe2876c88b97645d1e85542545b8020c27b022008d2a0f863e2583afde6397dfae2ec4b8fe61191bc74143f7686028674d45735012102fafc20310f52e9152f1e81ba76329d81211d54e4e1473dd4c70ab031e6cb5a2750661c00";
        let tx = client.decode_hex_tx(hex);
        assert!(tx.is_ok());
        let real_tx = tx.unwrap();
        let outputs = real_tx.output;
        assert_eq!(outputs.first().unwrap().value, 1000);
    }
}
