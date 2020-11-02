use super::{EstimateRequest, EstimateResult, EthereumClient, SendTransaction};
use crate::{
    common::{coins::CoinAmount, ethereum, Coin},
    utils::clone_into_array,
};
use ethereum_tx_sign::RawTransaction;
use std::convert::TryFrom;
use web3::{
    transports,
    types::{self, Block, BlockId, BlockNumber, CallRequest, Transaction, U256, U64},
    Web3,
};

/// A Web3 ethereum client
#[derive(Debug, Clone)]
pub struct Web3Client {
    web3: Web3<transports::Http>,
}

impl Web3Client {
    /// Create a new web3 ethereum client with the given transport
    pub fn new(transport: transports::Http) -> Self {
        let web3 = Web3::new(transport);
        Web3Client { web3 }
    }

    /// Create a web3 ethereum http client from a url
    pub fn url(url: &str) -> Result<Self, String> {
        let transport = transports::Http::new(url).map_err(|err| format!("{}", err))?;
        Ok(Web3Client::new(transport))
    }
}

#[async_trait]
impl EthereumClient for Web3Client {
    async fn get_latest_block_number(&self) -> Result<u64, String> {
        match self.web3.eth().block_number().await {
            Ok(result) => Ok(result.as_u64()),
            Err(err) => Err(format!("{}", err)),
        }
    }

    async fn get_transactions(&self, block_number: u64) -> Option<Vec<ethereum::Transaction>> {
        let block_number = BlockNumber::Number(U64([block_number]));
        let block: Option<Block<Transaction>> = match self
            .web3
            .eth()
            .block_with_txs(BlockId::Number(block_number))
            .await
        {
            Ok(result) => result,
            Err(error) => {
                debug!("[Web3] Failed to get block with txs: {}", error);
                return None;
            }
        };

        if let Some(block) = block {
            let txs = block
                .transactions
                .iter()
                .map(|tx| ethereum::Transaction {
                    hash: tx.hash.into(),
                    index: tx.transaction_index.unwrap().as_u64(),
                    block_number: tx.block_number.unwrap().as_u64(),
                    from: tx.from.into(),
                    to: tx.to.map(|val| val.into()),
                    value: tx.value.as_u128(),
                })
                .collect();
            return Some(txs);
        }

        None
    }

    async fn get_estimated_fee(&self, tx: &EstimateRequest) -> Result<EstimateResult, String> {
        if tx.amount.coin_type() != Coin::ETH {
            return Err(format!("Cannot get estimate for {}", tx.amount.coin_type()));
        }

        let gas_price: U256 = match self.web3.eth().gas_price().await {
            Ok(result) => result,
            Err(error) => {
                debug!("[Web3] Failed to get gas price for tx: {:?}", tx);
                return Err(format!("Failed to fetch gas price: {}", error));
            }
        };

        let gas_limit: U256 = match self
            .web3
            .eth()
            .estimate_gas(
                CallRequest {
                    from: Some(tx.from.into()),
                    to: Some(tx.to.into()),
                    gas: None,
                    gas_price: Some(gas_price),
                    value: Some(tx.amount.to_atomic().into()),
                    data: None,
                },
                None,
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                debug!("[Web3] Failed to get estimated gas for tx: {:?}", tx);
                return Err(format!("Failed to fetch gas limit: {}", error));
            }
        };

        let gas_price = match u128::try_from(gas_price) {
            Ok(price) => price,
            Err(_) => return Err("Gas price is over U128::MAX".to_owned()),
        };

        let gas_limit = match u128::try_from(gas_limit) {
            Ok(limit) => limit,
            Err(_) => return Err("Gas limit is over U128::MAX".to_owned()),
        };

        Ok(EstimateResult {
            gas_price,
            gas_limit,
        })
    }

    async fn get_balance(&self, address: ethereum::Address) -> Result<u128, String> {
        let balance = self
            .web3
            .eth()
            .balance(address.into(), Some(BlockNumber::Latest))
            .await
            .map_err(|err| err.to_string())?;

        let balance = match u128::try_from(balance) {
            Ok(balance) => balance,
            Err(_) => return Err("Balance is over U128::MAX".to_owned()),
        };

        Ok(balance)
    }

    async fn send(&self, tx: &SendTransaction) -> Result<ethereum::Hash, String> {
        if tx.amount.coin_type() != Coin::ETH {
            return Err(format!("Cannot send {}", tx.amount.coin_type()));
        }

        let chain_id: U256 = match self.web3.eth().chain_id().await {
            Ok(value) => value,
            Err(err) => return Err(format!("{}", err)),
        };

        let chain_id = u128::try_from(chain_id).map_err(|_| "Failed to get chain id".to_owned())?;
        let our_address = ethereum::Address::from(tx.from.public_key);

        let nonce: U256 = match self
            .web3
            .eth()
            .transaction_count(our_address.clone().into(), Some(BlockNumber::Pending))
            .await
        {
            Ok(value) => value,
            Err(err) => return Err(format!("{}", err)),
        };

        let raw_tx = RawTransaction {
            nonce: nonce,
            to: Some(tx.to.into()),
            value: U256::from(tx.amount.to_atomic()),
            gas_price: U256::from(tx.gas_price),
            gas: U256::from(tx.gas_limit),
            data: Vec::new(),
        };

        let our_secret = tx.from.private_key.to_string();
        let our_secret = hex::decode(our_secret).map_err(|_| "Failed to decode secret key")?;
        let our_secret: [u8; 32] = clone_into_array(&our_secret);
        let our_secret: types::H256 = our_secret.into();

        let signed_tx = raw_tx.sign(&our_secret, &chain_id);
        match self.web3.eth().send_raw_transaction(signed_tx.into()).await {
            Ok(hash) => Ok(hash.into()),
            Err(err) => {
                return Err(format!(
                    "{}, sender: {}, Tx: {:?}",
                    err, our_address, raw_tx,
                ))
            }
        }
    }
}

impl From<ethereum::Hash> for types::H256 {
    fn from(hash: ethereum::Hash) -> Self {
        hash.0.into()
    }
}

impl From<ethereum::Address> for types::H160 {
    fn from(address: ethereum::Address) -> Self {
        address.0.into()
    }
}

impl From<types::H256> for ethereum::Hash {
    fn from(hash: types::H256) -> Self {
        ethereum::Hash(hash.to_fixed_bytes())
    }
}

impl From<types::H160> for ethereum::Address {
    fn from(address: types::H160) -> Self {
        ethereum::Address(address.to_fixed_bytes())
    }
}

#[cfg(test)]
mod test {
    use ethereum::Address;
    use std::str::FromStr;

    use crate::common::GenericCoinAmount;

    use super::*;

    static WEB3_URL: &str = "https://api.myetherwallet.com/eth";

    #[tokio::test]
    async fn returns_latest_block_number() {
        let client = Web3Client::url(WEB3_URL).expect("Failed to create web3 client");
        assert!(client.get_latest_block_number().await.is_ok());
    }

    #[tokio::test]
    async fn returns_transactions() {
        let client = Web3Client::url(WEB3_URL).expect("Failed to create web3 client");

        let test_block_number = 10739404;
        // https://etherscan.io/block/10739404
        let transactions = client
            .get_transactions(test_block_number)
            .await
            .expect("Expected to get valid transactions");

        assert_eq!(transactions.len(), 179);

        // https://etherscan.io/tx/0x9fa1d1918e486e36f0066b76e812a6c8f8a2948d3055716e6e8c820f18e9e575
        let first = transactions
            .first()
            .expect("Expected to get a valid transaction");

        assert_eq!(first.index, 0);
        assert_eq!(first.block_number, test_block_number);
        assert_eq!(
            &first.hash.to_string(),
            "0x9fa1d1918e486e36f0066b76e812a6c8f8a2948d3055716e6e8c820f18e9e575"
        );
        assert_eq!(
            &first.from.to_string(),
            "0x6B17141D06d70B50AA4e8C263C0B4BA598c4b8a0"
        );
        assert_eq!(
            &first.to.as_ref().unwrap().to_string(),
            "0xdb50dBa4f9A046bfBE3D0D80E42308108A8Dc70a"
        );
        assert_eq!(first.value, 105403140000000000);
    }

    #[tokio::test]
    async fn returns_estimate() {
        let client = Web3Client::url(WEB3_URL).expect("Failed to create web3 client");
        let request = EstimateRequest {
            from: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(), // wEth address
            to: Address::from_str("0xdb50dBa4f9A046bfBE3D0D80E42308108A8Dc70a").unwrap(),
            amount: GenericCoinAmount::from_decimal_string(Coin::ETH, "1"),
        };

        let estimate = client.get_estimated_fee(&request).await.unwrap();
        assert_ne!(estimate.gas_limit, 0);
        assert_ne!(estimate.gas_price, 0);
    }

    #[tokio::test]
    async fn returns_balance() {
        let client = Web3Client::url(WEB3_URL).expect("Failed to create web3 client");

        let balance = client
            .get_balance(Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap())
            .await;
        assert!(balance.is_ok());
    }
}
