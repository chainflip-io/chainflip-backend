use super::refund::should_refund;

use crate::{
    common::{liquidity_provider::LiquidityProvider, Timestamp},
    transactions::QuoteTx,
    transactions::WitnessTx,
    transactions::{OutputTx, PoolChangeTx},
    utils::{price, primitives::U256},
    vault::transactions::memory_provider::FulfilledTxWrapper,
};

use std::{convert::TryFrom, error::Error, fmt};

/// Struct holding transactions
#[derive(Debug, PartialEq, Eq)]
pub struct SwapResult {
    /// The pool changes
    pub pool_changes: Vec<PoolChangeTx>,
    /// The output transactions
    pub output: OutputTx,
}

/// Errors for process_swap
#[derive(Debug, Eq, PartialEq)]
pub enum SwapError {
    /// Input amount overflowed
    InputAmountOverflow,
    /// Return address was not specified
    MissingReturnAddress,
    /// Failed to calculate output amount
    FailedToCalculateOutputAmount(String),
    /// Failed to generate output transaction
    FailedToGenerateOutputTx(String),
    /// Failed to generate pool change transactions
    FailedToGeneratePoolChangeTx,
    /// Missing witness transactions
    MissingWitnessTransactions,
}

impl fmt::Display for SwapError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SwapError::InputAmountOverflow => {
                write!(f, "Overflowed while calculating input amounts")
            }
            SwapError::MissingReturnAddress => {
                write!(f, "Cannot generate a refund. Return address is missing.")
            }
            SwapError::FailedToGenerateOutputTx(err) => {
                write!(f, "Failed to create output transaction: {}", err)
            }
            SwapError::FailedToGeneratePoolChangeTx => {
                write!(f, "Failed to create pool change transactions")
            }
            SwapError::FailedToCalculateOutputAmount(err) => {
                write!(f, "Failed to calculate output amount: {}", err)
            }
            SwapError::MissingWitnessTransactions => write!(f, "No witness transactions provided"),
        }
    }
}

impl Error for SwapError {}

/// Process a given swap quote
pub fn process_swap<L: LiquidityProvider>(
    provider: &L,
    quote: &FulfilledTxWrapper<QuoteTx>,
    witnesses: &[WitnessTx],
) -> Result<SwapResult, SwapError> {
    if witnesses.is_empty() {
        return Err(SwapError::MissingWitnessTransactions);
    }

    let quote_id = quote.inner.id;
    let witness_ids = witnesses.iter().map(|w| w.id).collect();

    // Calculate input amounts
    let input_amount = witnesses.iter().fold(U256::from(0), |acc, tx| {
        // use saturating add to make it easier since we're going to try convert this back to u128 anyway
        acc.saturating_add(U256::from(tx.amount))
    });

    let input_amount = match u128::try_from(input_amount) {
        Ok(amount) => amount,
        Err(_) => return Err(SwapError::InputAmountOverflow),
    };

    // Calculate the outputs from the inputted witness amounts
    let output = match price::get_output(
        provider,
        quote.inner.input,
        input_amount,
        quote.inner.output,
    ) {
        Ok(calc) => calc,
        Err(err) => return Err(SwapError::FailedToCalculateOutputAmount(err.to_owned())),
    };

    let output_amount = output.second.unwrap_or(output.first).output_amount;
    /*
       There are a lot of things which can happen when we process swaps.
       The main logic can be broken down to the following:
        - Do we have a return address?
            Yes: Check if we need to return the funds
            No: Make the swap regardless of user options
    */
    if should_refund(quote, input_amount, output_amount) {
        // Refund the user

        let return_address = match quote.inner.return_address.clone() {
            Some(address) => address,
            None => return Err(SwapError::MissingReturnAddress),
        };

        let output = match OutputTx::new(
            Timestamp::now(),
            quote_id,
            witness_ids,
            vec![],
            quote.inner.input,
            return_address,
            input_amount,
        ) {
            Ok(output) => output,
            Err(err) => return Err(SwapError::FailedToGenerateOutputTx(err.to_owned())),
        };

        return Ok(SwapResult {
            pool_changes: vec![],
            output,
        });
    }

    assert!(output_amount > 0);

    // Construct the pool change transactions
    let pool_changes: Vec<PoolChangeTx> = [Some(output.first), output.second]
        .iter()
        .filter_map(|x| {
            if let Some(detail) = x {
                match detail.to_pool_change_tx() {
                    Ok(tx) => Some(tx),
                    Err(error) => {
                        error!(
                            "Failed to create pool change transactions for {:?}. Error: {}",
                            detail, error
                        );
                        None
                    }
                }
            } else {
                None
            }
        })
        .collect();

    if pool_changes.is_empty() {
        return Err(SwapError::FailedToGeneratePoolChangeTx);
    }

    // Create the output
    let pool_change_ids = pool_changes.iter().map(|tx| tx.id).collect();
    let output = match OutputTx::new(
        Timestamp::now(),
        quote_id,
        witness_ids,
        pool_change_ids,
        quote.inner.output,
        quote.inner.output_address.clone(),
        output_amount,
    ) {
        Ok(output) => output,
        Err(err) => return Err(SwapError::FailedToGenerateOutputTx(err.to_owned())),
    };

    Ok(SwapResult {
        pool_changes,
        output,
    })
}

#[cfg(test)]
mod test {
    use uuid::Uuid;

    use super::*;
    use crate::{
        common::coins::{Coin, CoinAmount, GenericCoinAmount, PoolCoin},
        common::liquidity_provider::{Liquidity, MemoryLiquidityProvider},
        common::WalletAddress,
    };

    fn to_atomic(coin: Coin, amount: &str) -> u128 {
        GenericCoinAmount::from_decimal_string(coin, amount).to_atomic()
    }

    fn setup() -> (
        MemoryLiquidityProvider,
        FulfilledTxWrapper<QuoteTx>,
        Vec<WitnessTx>,
    ) {
        let mut provider = MemoryLiquidityProvider::new();
        provider.set_liquidity(
            PoolCoin::ETH,
            Some(Liquidity::new(
                to_atomic(Coin::ETH, "10000.0"),
                to_atomic(Coin::LOKI, "20000.0"),
            )),
        );

        let eth_address = WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5");
        let loki_address = WalletAddress::new("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY");

        let quote = QuoteTx::new(
            Timestamp::now(),
            Coin::ETH,
            eth_address.clone(),
            "7".to_string(),
            Some(eth_address),
            Coin::LOKI,
            loki_address,
            1,
            None,
        )
        .expect("Expected valid quote");

        let quote = FulfilledTxWrapper {
            inner: quote,
            fulfilled: false,
        };

        let witness_txes = vec![
            WitnessTx {
                id: Uuid::new_v4(),
                timestamp: Timestamp::now(),
                quote_id: quote.inner.id,
                transaction_id: Uuid::new_v4().to_string(),
                transaction_block_number: 0,
                transaction_index: 0,
                amount: to_atomic(Coin::ETH, "1500.0"),
                coin: Coin::ETH,
            },
            WitnessTx {
                id: Uuid::new_v4(),
                timestamp: Timestamp::now(),
                quote_id: quote.inner.id,
                transaction_id: Uuid::new_v4().to_string(),
                transaction_block_number: 0,
                transaction_index: 0,
                amount: to_atomic(Coin::ETH, "1000.0"),
                coin: Coin::ETH,
            },
        ];

        (provider, quote, witness_txes)
    }

    #[test]
    fn returns_refunds() {
        // Trigger a refund by setting quote to fulfilled
        let (provider, mut quote, witnesses) = setup();
        quote.fulfilled = true;

        let result =
            process_swap(&provider, &quote, &witnesses).expect("Expected to process a swap");

        let amount = witnesses.iter().fold(0, |acc, tx| acc + tx.amount);

        assert!(result.pool_changes.is_empty());

        let witness_ids: Vec<Uuid> = witnesses.iter().map(|tx| tx.id).collect();

        let output = result.output;
        assert_eq!(output.quote_tx, quote.inner.id);
        assert_eq!(output.witness_txs, witness_ids);
        assert!(
            output.timestamp.0 >= quote.inner.timestamp.0,
            "Expected output timestamp to be newer than quote"
        );
        assert_eq!(output.pool_change_txs.len(), 0);
        assert_eq!(output.coin, Coin::ETH);
        assert_eq!(output.address, quote.inner.return_address.unwrap());
        assert_eq!(output.amount, amount);
    }

    #[test]
    fn refunds_if_no_liquidity() {
        let (mut provider, quote, witnesses) = setup();
        provider.set_liquidity(PoolCoin::ETH, None);

        let result =
            process_swap(&provider, &quote, &witnesses).expect("Expected to process a swap");

        let amount = witnesses.iter().fold(0, |acc, tx| acc + tx.amount);

        assert!(result.pool_changes.is_empty());

        let output = result.output;
        assert_eq!(output.pool_change_txs.len(), 0);
        assert_eq!(output.coin, Coin::ETH);
        assert_eq!(output.amount, amount);
    }

    #[test]
    fn returns_swaps() {
        let (provider, quote, witnesses) = setup();

        assert!(provider.get_liquidity(PoolCoin::ETH).is_some());

        let result =
            process_swap(&provider, &quote, &witnesses).expect("Expected to process a swap");

        let pool_changes = result.pool_changes;
        assert_eq!(pool_changes.len(), 1);

        let change = pool_changes.first().unwrap();
        assert_eq!(change.coin, PoolCoin::ETH);
        assert_eq!(change.depth_change, to_atomic(Coin::ETH, "2500.0") as i128);
        assert_eq!(
            change.loki_depth_change,
            -1 * to_atomic(Coin::LOKI, "3199.5") as i128
        );

        let witness_ids: Vec<Uuid> = witnesses.iter().map(|tx| tx.id).collect();

        let output = result.output;
        assert_eq!(output.quote_tx, quote.inner.id);
        assert_eq!(output.witness_txs, witness_ids);
        assert!(
            output.timestamp.0 >= quote.inner.timestamp.0,
            "Expected output timestamp to be newer than quote"
        );
        assert_eq!(output.pool_change_txs.len(), 1);
        assert_eq!(output.pool_change_txs, vec![change.id]);
        assert_eq!(output.coin, Coin::LOKI);
        assert_eq!(output.address, quote.inner.output_address);
        assert_eq!(output.amount, to_atomic(Coin::LOKI, "3199.5"));
    }

    #[test]
    fn returns_correct_swaps_for_non_loki_quotes() {
        let (mut provider, mut quote, witnesses) = setup();
        quote.inner.output = Coin::BTC;
        quote.inner.output_address = WalletAddress::new("1FPR9qMV6nikLxKP1MnZG6jh4viqFUs7wV");

        assert!(provider.get_liquidity(PoolCoin::ETH).is_some());

        provider.set_liquidity(
            PoolCoin::BTC,
            Some(Liquidity::new(
                to_atomic(Coin::BTC, "12769.0"),
                to_atomic(Coin::LOKI, "10191.0"),
            )),
        );

        let result =
            process_swap(&provider, &quote, &witnesses).expect("Expected to process a swap");

        let pool_changes = result.pool_changes;
        assert_eq!(pool_changes.len(), 2);

        let first_change = pool_changes.first().unwrap();
        assert_eq!(first_change.coin, PoolCoin::ETH);
        assert_eq!(
            first_change.depth_change,
            to_atomic(Coin::ETH, "2500.0") as i128
        );
        assert_eq!(
            first_change.loki_depth_change,
            -1 * to_atomic(Coin::LOKI, "3199.5") as i128
        );

        let second_change = pool_changes.last().unwrap();
        assert_eq!(second_change.coin, PoolCoin::BTC);
        assert_eq!(
            second_change.loki_depth_change,
            to_atomic(Coin::LOKI, "3199.5") as i128
        );
        assert_eq!(
            second_change.depth_change,
            -1 * to_atomic(Coin::BTC, "2322.0") as i128
        );

        let witness_ids: Vec<Uuid> = witnesses.iter().map(|tx| tx.id).collect();

        let output = result.output;
        assert_eq!(output.quote_tx, quote.inner.id);
        assert_eq!(output.witness_txs, witness_ids);
        assert!(
            output.timestamp.0 >= quote.inner.timestamp.0,
            "Expected output timestamp to be newer than quote"
        );
        assert_eq!(
            output.pool_change_txs,
            vec![first_change.id, second_change.id]
        );
        assert_eq!(output.coin, Coin::BTC);
        assert_eq!(output.address, quote.inner.output_address);
        assert_eq!(output.amount, to_atomic(Coin::BTC, "2322.0"));
    }

    #[test]
    fn returns_error_if_no_liquidity_and_no_refund() {
        let (mut provider, mut quote, witnesses) = setup();
        quote.inner.return_address = None; // Refund can only happen with a return address
        provider.set_liquidity(PoolCoin::ETH, None);

        let result = process_swap(&provider, &quote, &witnesses);
        assert_eq!(result.unwrap_err(), SwapError::MissingReturnAddress);
    }

    #[test]
    fn returns_error_on_invalid_return_address() {
        let (provider, mut quote, witnesses) = setup();
        quote.inner.return_address = Some(WalletAddress::new("Invalid address"));
        quote.fulfilled = true;

        let result = process_swap(&provider, &quote, &witnesses);
        assert_eq!(
            result.unwrap_err(),
            SwapError::FailedToGenerateOutputTx("Invalid output address".to_owned())
        );
    }

    #[test]
    fn returns_error_on_invalid_output_address() {
        let (provider, mut quote, witnesses) = setup();
        quote.inner.output_address = WalletAddress::new("Invalid address");

        let result = process_swap(&provider, &quote, &witnesses);
        assert_eq!(
            result.unwrap_err(),
            SwapError::FailedToGenerateOutputTx("Invalid output address".to_owned())
        );
    }

    #[test]
    fn returns_error_if_input_amounts_overflow() {
        let (provider, quote, mut witnesses) = setup();

        let first = witnesses.first_mut().unwrap();
        first.amount = u128::MAX;

        let result = process_swap(&provider, &quote, &witnesses);
        assert_eq!(result.unwrap_err(), SwapError::InputAmountOverflow);
    }

    #[test]
    fn returns_error_if_no_witnesses() {
        let (provider, quote, _) = setup();

        let empty = vec![];

        let result = process_swap(&provider, &quote, &empty);
        assert_eq!(result.unwrap_err(), SwapError::MissingWitnessTransactions);
    }

    #[test]
    fn returns_error_if_output_calculation_failed() {
        let (provider, mut quote, witnesses) = setup();
        quote.inner.output = quote.inner.input;
        quote.inner.output_address = quote.inner.input_address.clone();

        let result = process_swap(&provider, &quote, &witnesses);
        assert_eq!(
            result.unwrap_err(),
            SwapError::FailedToCalculateOutputAmount(
                "Cannot get output amount for the same coin".to_owned()
            )
        );
    }
}
