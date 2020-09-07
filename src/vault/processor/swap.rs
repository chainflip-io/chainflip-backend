use super::utils::is_swap_quote_expired;
use crate::{
    common::{Coin, WalletAddress},
    transactions::{PoolChangeTx, QuoteTx, WitnessTx},
    vault::transactions::{memory_provider::FulfilledTxWrapper, TransactionProvider},
};

struct SendResult {
    quote: QuoteTx,
    witnesses: Vec<WitnessTx>,
    pool_changes: Vec<PoolChangeTx>,
    coin: Coin,
    address: WalletAddress,
    amount: u128,
}

impl SendResult {
    /// Create a swap.
    fn swap(
        quote: QuoteTx,
        witnesses: Vec<WitnessTx>,
        pool_changes: Vec<PoolChangeTx>,
        amount: u128,
    ) -> Self {
        let address = quote.output_address.clone();
        let coin = quote.output.clone();
        SendResult {
            quote,
            witnesses,
            pool_changes,
            coin,
            address,
            amount,
        }
    }

    /// Refund all the witnesses amounts
    fn refund(quote: QuoteTx, witnesses: Vec<WitnessTx>) -> Result<Self, &'static str> {
        let address = match &quote.return_address {
            Some(address) => address.clone(),
            None => {
                return Err("Return address not found");
            }
        };

        let coin = quote.input.clone();
        let amount = witnesses.iter().fold(0, |acc, tx| acc + tx.amount);

        Ok(SendResult {
            quote,
            witnesses,
            pool_changes: vec![],
            coin,
            address,
            amount,
        })
    }
}

impl SendResult {
    /// Wether this is a refund
    fn is_refund(&self) -> bool {
        self.quote.input == self.coin
    }
}

/// Wether we should refund the user
fn should_refund<T: TransactionProvider>(
    provider: &T,
    quote: &FulfilledTxWrapper<QuoteTx>,
    witnesses: &[WitnessTx],
) -> bool {
    /*
        If we have a refund address then we should refund the user if:
            - Quote was already fulfilled
            - Quote has expired
            - Slippage limit is set and slippage between quote effective price and current effective price is greater than the slippage limit
    */
    if quote.inner.return_address.is_none() {
        return false;
    }

    if quote.fulfilled || is_swap_quote_expired(&quote.inner) {
        return true;
    }

    // Slippage limit of 0 means we swap regardless of the limit
    let slippage_limit = quote.inner.slippage_limit;
    if slippage_limit <= 0.0 {
        return false;
    }

    let input_amount = witnesses.iter().fold(0, |acc, tx| acc + tx.amount);
    let output = provider
        .get_output_amount(quote.inner.input, input_amount, quote.inner.output)
        .expect("Failed to get output amount when processing swap");

    let output_amount = output.second.unwrap_or(output.first).output_amount;
    if output_amount == 0 {
        return true;
    }

    let effective_price = input_amount as f64 / output_amount as f64;

    // Calculate the slippage.
    // This will return negative value if we get a better price than what was quoted.
    let slippage = 1.0 - (quote.inner.effective_price / effective_price);

    slippage > slippage_limit as f64
}

/// Process a given swap quote
fn process_swap<T: TransactionProvider>(
    provider: &T,
    quote: &FulfilledTxWrapper<QuoteTx>,
    witnesses: &[WitnessTx],
) -> Result<SendResult, &'static str> {
    /*
       There are a lot of things which can happen when we process swaps.
       The main logic can be broken down to the following:
        - Do we have a return address?
            Yes: Check if we need to return the funds
            No: Make the swap regardless of user options

        If we have a refund address then we should refund the user if:
            - Quote was already fulfilled
            - Quote has expired
            - Slippage between quote effective price and current effective price is greater than the slippage limit

        In all other cases we should swap all the sent funds.
    */
    if should_refund(provider, quote, witnesses) {
        // Refund the user
        return match SendResult::refund(quote.inner.clone(), witnesses.into()) {
            Ok(refund) => Ok(refund),
            Err(_) => {
                error!(
                    "[Process Swap] Failed to generate refund for {:?} and {:?}",
                    quote.inner, witnesses
                );
                return Err("Failed to generate refund");
            }
        };
    }

    // Calculate the outputs from the inputted witness amounts
    let input_amount = witnesses.iter().fold(0, |acc, tx| acc + tx.amount);
    let output = provider.get_output_amount(quote.inner.input, input_amount, quote.inner.output)?;

    let output_amount = output.second.unwrap_or(output.first).output_amount;
    if output_amount == 0 {
        warn!(
            "Output amount of 0 detected for {:?}. Cannot refund it!!!",
            quote.inner
        );
        return Err("Found 0 output amount");
    }

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
        return Err("Failed to create pool change transactions");
    }

    // Build the swap
    Ok(SendResult::swap(
        quote.inner.clone(),
        Vec::from(witnesses),
        pool_changes,
        output_amount,
    ))
}
