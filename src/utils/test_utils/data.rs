use super::{
    staking::get_random_staker, TEST_BTC_ADDRESS, TEST_ETH_ADDRESS, TEST_ETH_SALT,
    TEST_LOKI_ADDRESS, TEST_LOKI_PAYMENT_ID,
};
use crate::{
    common::{Staker, StakerId},
    utils::calculate_effective_price,
};
use chainflip_common::types::{
    chain::*, coin::Coin, fraction::WithdrawFraction, Network, Timestamp,
};
use uuid::Uuid;

/// Struct for generating test data
pub struct TestData {}

impl TestData {
    /// Create a fake deposit quote for a random staker
    pub fn deposit_quote(pool: Coin) -> DepositQuote {
        Self::deposit_quote_for_id(get_random_staker().id(), pool)
    }

    /// Create a fake deposit quote for the given staker
    pub fn deposit_quote_for_id(staker_id: StakerId, pool: Coin) -> DepositQuote {
        let address = match pool {
            Coin::BTC => TEST_BTC_ADDRESS,
            Coin::ETH => TEST_ETH_ADDRESS,
            _ => panic!("Failed to create fake deposit quote"),
        };

        let coin_input_address_id = match pool {
            Coin::BTC => 6u32.to_be_bytes().to_vec(),
            Coin::ETH => TEST_ETH_SALT.to_vec(),
            _ => panic!("Failed to create fake deposit quote"),
        };

        let quote = DepositQuote {
            timestamp: Timestamp::now(),
            pool,
            staker_id: staker_id.bytes().to_vec(),
            coin_input_address: address.into(),
            coin_input_address_id,
            coin_return_address: address.into(),
            base_input_address: TEST_LOKI_ADDRESS.into(),
            base_input_address_id: TEST_LOKI_PAYMENT_ID.to_vec(),
            base_return_address: TEST_LOKI_ADDRESS.into(),
            event_number: None,
        };
        quote.validate(Network::Testnet).unwrap();
        quote
    }

    /// Create a fake witness
    pub fn witness(quote_id: UniqueId, amount: u128, coin: Coin) -> Witness {
        let witness = Witness {
            quote: quote_id,
            // Just create a random pseudo-transaction_id for the witness
            transaction_id: Uuid::new_v4().to_string().into(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount,
            coin,
            event_number: None,
            confirmed: false,
        };
        witness.validate(Network::Testnet).unwrap();
        witness
    }

    /// Create a fake witness with an event num
    pub fn witness_with_event_num(
        quote_id: UniqueId,
        amount: u128,
        coin: Coin,
        event_num: u64,
    ) -> Witness {
        let witness = Witness {
            quote: quote_id,
            transaction_id: "".into(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount,
            coin,
            event_number: Some(event_num),
            confirmed: false,
        };
        witness.validate(Network::Testnet).unwrap();
        witness
    }

    /// Create a fake withdraw request for the given staker
    pub fn withdraw_request_for_staker(staker: &Staker, pool: Coin) -> WithdrawRequest {
        let staker_id = staker.id();

        let mut request = WithdrawRequest {
            timestamp: Timestamp::now(),
            staker_id: staker_id.bytes().to_vec(),
            pool,
            base_address: TEST_LOKI_ADDRESS.into(),
            other_address: TEST_ETH_ADDRESS.into(),
            fraction: WithdrawFraction::MAX,
            signature: vec![],
            event_number: None,
        };

        request
            .sign(&staker.keys)
            .expect("could not sign withdraw request");

        request.validate(Network::Testnet).unwrap();
        request
    }

    /// Create a fake swap quote
    pub fn swap_quote(input: Coin, output: Coin) -> SwapQuote {
        let input_address = match input {
            Coin::LOKI => TEST_LOKI_ADDRESS,
            Coin::ETH => TEST_ETH_ADDRESS,
            Coin::BTC => TEST_BTC_ADDRESS,
        };

        let input_address_id = match input {
            Coin::LOKI => TEST_LOKI_PAYMENT_ID.to_vec(),
            Coin::ETH => TEST_ETH_SALT.to_vec(),
            Coin::BTC => 7u32.to_be_bytes().to_vec(),
        };

        let output_address = match output {
            Coin::LOKI => TEST_LOKI_ADDRESS,
            Coin::ETH => TEST_ETH_ADDRESS,
            Coin::BTC => TEST_BTC_ADDRESS,
        };

        let quote = SwapQuote {
            timestamp: Timestamp::now(),
            input,
            input_address: input_address.into(),
            input_address_id,
            return_address: Some(input_address.into()),
            output,
            output_address: output_address.into(),
            effective_price: calculate_effective_price(1, 1).unwrap(),
            slippage_limit: None,
            event_number: None,
        };
        quote.validate(Network::Testnet).unwrap();
        quote
    }

    /// Create a fake pool change
    pub fn pool_change(pool: Coin, depth_change: i128, base_depth_change: i128) -> PoolChange {
        let change = PoolChange::new(pool, depth_change, base_depth_change, None);
        change.validate(Network::Testnet).unwrap();
        change
    }

    /// Create a fake output
    pub fn output(coin: Coin, amount: u128) -> Output {
        let address = match coin {
            Coin::LOKI => TEST_LOKI_ADDRESS,
            Coin::ETH => TEST_ETH_ADDRESS,
            Coin::BTC => TEST_BTC_ADDRESS,
        };

        let output = Output {
            parent: OutputParent::SwapQuote(0),
            witnesses: vec![],
            pool_changes: vec![],
            coin,
            address: address.into(),
            amount,
            event_number: None,
        };
        output.validate(Network::Testnet).unwrap();
        output
    }

    /// Create a fake output sent
    pub fn output_sent(coin: Coin) -> OutputSent {
        let address = match coin {
            Coin::LOKI => TEST_LOKI_ADDRESS,
            Coin::ETH => TEST_ETH_ADDRESS,
            Coin::BTC => TEST_BTC_ADDRESS,
        };

        let sent = OutputSent {
            outputs: vec![1],
            coin,
            address: address.into(),
            amount: 100,
            fee: 0,
            transaction_id: "txid".into(),
            event_number: None,
        };
        sent.validate(Network::Testnet).unwrap();
        sent
    }
}
