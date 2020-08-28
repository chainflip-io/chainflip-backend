//! Bindings to some commonly used methods exposed by Loki RPC Wallet

use crate::common::{
    coins::{CoinAmount, LokiAmount},
    LokiPaymentId, LokiWalletAddress,
};
use std::convert::{TryFrom, TryInto};
use std::fmt;

use std::str::FromStr;

use serde::{Deserialize, Serialize};

// get_bulk_payments

/// Error returned by the rpc wallet
#[derive(Debug, Deserialize)]
pub struct LokiResponseError {
    code: i32,
    message: String,
}

/// Response wrapper used in all responses from the rpc wallet
#[derive(Debug, Deserialize)]
pub struct LokiResponse {
    error: Option<LokiResponseError>,
    result: Option<serde_json::Value>,
}

/// Response for endpoint: `balance`
#[derive(Debug, Deserialize)]
struct BalanceResponse {
    balance: u128,
    blocks_to_unlock: u32,
    multisig_import_needed: bool,
    unlocked_balance: u128,
}

async fn send_req_inner(
    port: u16,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();

    let url = format!("http://localhost:{}/json_rpc", port);

    debug!(
        "Loki wallet rpc: /{}. Sending params: {}",
        method,
        params.to_string()
    );

    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "0",
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

    let res: LokiResponse = serde_json::from_str(&text).map_err(|err| err.to_string())?;

    if let Some(err) = res.error {
        error!("Loki wallet rpc error");
        return Err(err.message.to_owned());
    }

    if let Some(result) = res.result {
        Ok(result)
    } else {
        Err("Neither result no error present in response".to_owned())
    }
}

/// Response for endpoint: `make_integrated_address`
#[derive(Debug, Deserialize)]
pub struct IntegratedAddressResponse {
    /// The address that can be used to transfer loki to
    pub integrated_address: String,
    /// Payment identifier
    pub payment_id: String,
}

/// Make an integrated address from an optional `payment_id`. If `payment_id` is not specified,
/// a random one should be created by the wallet.
pub async fn make_integrated_address(
    port: u16,
    payment_id: Option<&str>,
) -> Result<IntegratedAddressResponse, String> {
    let mut params = serde_json::json!({});

    if let Some(payment_id) = payment_id {
        params["payment_id"] = payment_id.to_string().into();
    }

    let res = send_req_inner(port, "make_integrated_address", params)
        .await
        .map_err(|err| err.to_string())?;

    let address: IntegratedAddressResponse =
        serde_json::from_value(res).map_err(|err| err.to_string())?;

    Ok(address)
}

/// Make `get_transfers` call
pub async fn get_all_transfers(port: u16) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "in": true,
        "all_accounts": true
    });

    let res = send_req_inner(port, "get_transfers", params).await;

    return res;
}

/// Major and minor indexes (account and subaddress indexes, respectively)
#[derive(Debug, Deserialize)]
pub struct SubaddressIndex {
    /// The account index
    major: u32,
    /// The subaddress index
    minor: u32,
}

/// Payment entry as received from loki wallet
#[derive(Debug, Deserialize)]
pub struct BulkPaymentResponseEntryRaw {
    /// Payment Id matching the input parameter
    payment_id: String,
    /// Transaction hash used as the transaction Id
    tx_hash: String,
    // Amount for this payment (in atomic units)
    amount: u64,
    /// Height of the block that first confirmed this payment
    block_height: u64,
    /// Time (in blocks) until this payment is safe to spend
    unlock_time: u64,
    /// Account and subaddress indexes
    subaddr_index: SubaddressIndex,
    /// Address receiving the payment
    address: String,
}

/// Bulk payment response as received from loki wallet
#[derive(Debug, Deserialize)]
pub struct BulkPaymentResponseRaw {
    /// List of payment details
    payments: Vec<BulkPaymentResponseEntryRaw>,
}

impl TryFrom<BulkPaymentResponseEntryRaw> for BulkPaymentResponseEntry {
    type Error = String;

    fn try_from(a: BulkPaymentResponseEntryRaw) -> Result<Self, Self::Error> {
        let entry = BulkPaymentResponseEntry {
            payment_id: LokiPaymentId::from_str(&a.payment_id)?,
            tx_hash: a.tx_hash,
            amount: LokiAmount::from_atomic(a.amount as u128),
            block_height: a.block_height,
            unlock_time: a.unlock_time,
            subaddr_index: a.subaddr_index,
            address: LokiWalletAddress::from_str(&a.address)?,
        };

        Ok(entry)
    }
}

impl TryFrom<BulkPaymentResponseRaw> for BulkPaymentResponse {
    type Error = String;

    fn try_from(a: BulkPaymentResponseRaw) -> Result<Self, Self::Error> {
        let payments = a
            .payments
            .into_iter()
            .map(|x| x.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        let res = BulkPaymentResponse { payments };

        Ok(res)
    }
}

/// Payment entry
#[derive(Debug)]
pub struct BulkPaymentResponseEntry {
    /// Payment Id matching the input parameter
    pub payment_id: LokiPaymentId,
    /// Transaction hash used as the transaction Id
    tx_hash: String,
    // Amount for this payment
    pub amount: LokiAmount,
    /// Height of the block that first confirmed this payment
    block_height: u64,
    /// Time (in blocks) until this payment is safe to spend
    unlock_time: u64,
    /// Account and subaddress indexes
    subaddr_index: SubaddressIndex,
    /// Address receiving the payment
    address: LokiWalletAddress,
}

/// Bulk payment reponse
#[derive(Debug)]
pub struct BulkPaymentResponse {
    /// List of payment details
    pub payments: Vec<BulkPaymentResponseEntry>,
}

/// Check whether equals to {}
fn is_empty_object(v: &serde_json::Value) -> bool {
    if v.is_object() {
        if v.as_object().unwrap().len() == 0 {
            return true;
        }
    }

    false
}

/// Get all payments for given payment ids (Uses `get_bulk_payments` endpoint)
pub async fn get_bulk_payments(
    port: u16,
    payment_ids: Vec<LokiPaymentId>,
    min_block_height: u64,
) -> Result<BulkPaymentResponse, String> {
    let payment_ids = serde_json::to_value(payment_ids).map_err(|err| err.to_string())?;

    let params = serde_json::json!({
        "payment_ids": payment_ids,
        "min_block_height": min_block_height
    });

    let res = send_req_inner(port, "get_bulk_payments", params).await?;

    // Instead of reponding with an empty list, loki gives an empty response...

    let res = if is_empty_object(&res) {
        BulkPaymentResponseRaw { payments: vec![] }
    } else {
        serde_json::from_value(res).map_err(|err| err.to_string())?
    };

    res.try_into()
}

/// Balance in a loki wallet
#[derive(Debug)]
pub struct LokiBalance {
    /// Total balance (including locked inputs)
    pub balance: LokiAmount,
    /// Unlocked balance
    pub unlocked_balance: LokiAmount,
    /// Number of blocks until all balance becomes unlocked (ready to spend)
    pub blocks_to_unlock: u32,
}

impl fmt::Display for LokiBalance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ Balance: {}, Unlocked: {}, blocks to unlock: {}}}",
            self.balance, self.unlocked_balance, self.blocks_to_unlock
        )
    }
}

#[derive(Deserialize)]
struct HeightResponse {
    // Blockchain height
    height: u64,
}

/// Request blockchain height from wallet
pub async fn get_height(port: u16) -> Result<u64, String> {
    let params = serde_json::json!({});

    let res = send_req_inner(port, "get_height", params).await?;

    let res: HeightResponse = serde_json::from_value(res).map_err(|err| err.to_string())?;

    Ok(res.height)
}

/// Request balance in the default account
pub async fn get_balance(port: u16) -> Result<LokiBalance, String> {
    let params = serde_json::json!({
        "account_index": 0,
    });

    let res = send_req_inner(port, "get_balance", params)
        .await
        .map_err(|err| err.to_string())?;

    let balance_res: BalanceResponse =
        serde_json::from_value(res).map_err(|err| err.to_string())?;

    let balance = LokiBalance {
        balance: LokiAmount::from_atomic(balance_res.balance),
        unlocked_balance: LokiAmount::from_atomic(balance_res.unlocked_balance),
        blocks_to_unlock: balance_res.blocks_to_unlock,
    };

    Ok(balance)
}

#[derive(Debug, Serialize)]
struct Destination {
    /// Amount to send in atomic units
    amount: u64,
    /// Destination public address
    address: String,
}

// From loki-rpc-wallet:
// std::list<wallet::transfer_destination> destinations; // Array of destinations to receive LOKI.
// uint32_t account_index;                       // (Optional) Transfer from this account index. (Defaults to 0)
// std::set<uint32_t> subaddr_indices;           // (Optional) Transfer from this set of subaddresses. (Defaults to 0)
// uint32_t priority;                            // Set a priority for the transaction. Accepted values are: 1 for unimportant or 5 for blink.  (0 and 2-4 are accepted for backwards compatibility and are equivalent to 5)
// bool blink;                                   // (Deprecated) Set priority to 5 for blink, field is deprecated: specifies that the tx should be blinked (`priority` will be ignored).
// uint64_t unlock_time;                         // Number of blocks before the loki can be spent (0 to use the default lock time).
// std::string payment_id;                       // (Optional) Random 64-character hex string to identify a transaction.
// bool get_tx_key;                              // (Optional) Return the transaction key after sending.
// bool do_not_relay;                            // (Optional) If true, the newly created transaction will not be relayed to the loki network. (Defaults to false)
// bool get_tx_hex;                              // Return the transaction as hex string after sending. (Defaults to false)
// bool get_tx_metadata;

#[derive(Debug, Serialize)]
struct TransferRequestParams {
    /// List of destinations
    destinations: Vec<Destination>,
    /// Priority value for the transaction. Accepted values are: 1 for unimportant or 5 for blink.
    priority: u8,
    /// Number of blocks before the loki can be spent (0 to use the default lock time).
    unlock_time: u64,
    // /// Random 64-character hex (not 16?) string to identify a transaction
    #[serde(skip_serializing_if = "Option::is_none")]
    payment_id: Option<String>,
    get_tx_hex: bool,
    get_tx_metadata: bool,
    get_tx_key: bool,
}

/// Transfer response as received from loki wallet
#[derive(Deserialize)]
struct TransferResponseRaw {
    /// Fee in atomic units
    fee: u64,
}

/// User-friendly transfer response
#[derive(Debug)]
pub struct TransferResponse {
    /// Fee as typed amount
    pub fee: LokiAmount,
}

/// Make an rpc command to transfer `amount` of loki to `address`
pub async fn transfer(
    port: u16,
    amount: &LokiAmount,
    address: &LokiWalletAddress,
    payment_id: Option<&str>,
) -> Result<TransferResponse, String> {
    let amount: u64 = u64::try_from(amount.to_atomic()).map_err(|e| e.to_string())?;

    let dest = Destination {
        amount,
        address: address.to_str().to_owned(),
    };

    let params = TransferRequestParams {
        destinations: vec![dest],
        priority: 5, // 5 for blink transactions
        unlock_time: 0,
        payment_id: payment_id.map(ToOwned::to_owned),
        get_tx_key: true,
        get_tx_hex: true,
        get_tx_metadata: true,
    };

    let params = serde_json::to_value(&params).map_err(|err| err.to_string())?;

    info!("Params: {}", serde_json::to_string_pretty(&params).unwrap());

    let res = send_req_inner(port, "transfer", params)
        .await
        .map_err(|err| err.to_string())?;

    let res: TransferResponseRaw = serde_json::from_value(res).map_err(|err| err.to_string())?;

    // Note that we will never get values larger than u64::MAX from the wallet...
    let res = TransferResponse {
        fee: LokiAmount::from_atomic(res.fee as u128),
    };

    Ok(res)
}
