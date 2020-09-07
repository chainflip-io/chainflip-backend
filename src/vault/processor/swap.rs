use crate::{
    common::{Coin, WalletAddress},
    transactions::{PoolChangeTx, QuoteTx, WitnessTx},
};
use itertools::Itertools;

struct SendResult {
    quote: QuoteTx,
    witnesses: Vec<WitnessTx>,
    pool_changes: Vec<PoolChangeTx>,
    coin: Coin,
    address: WalletAddress,
    amount: u128,
}

impl SendResult {
    /// Create a swap result
    fn swap(
        quote: QuoteTx,
        witnesses: Vec<WitnessTx>,
        pool_changes: Vec<PoolChangeTx>,
        amount: u128,
    ) -> Self {
        SendResult {
            quote: quote.clone(),
            witnesses,
            pool_changes,
            coin: quote.output,
            address: quote.output_address,
            amount,
        }
    }

    /// Create a refund result
    fn refund(
        quote: QuoteTx,
        witnesses: Vec<WitnessTx>,
        amount: u128,
    ) -> Result<Self, &'static str> {
        // Try and determine the return address
        let address = match quote.return_address {
            Some(address) => address,
            None => {
                // Go through witnesses and pull out the witness address
                let addresses: Vec<WalletAddress> = witnesses
                    .iter()
                    .map(|tx| tx.sender.unwrap())
                    .unique()
                    .collect();

                if addresses.len() != 1 {
                    return Err(
                        "Return address cannot be determined because multiple addresses are found.",
                    );
                }

                addresses.first().cloned().unwrap()
            }
        };

        Ok(SendResult {
            quote: quote.clone(),
            witnesses,
            pool_changes: vec![],
            coin: quote.input,
            address,
            amount,
        })
    }
}

/// Process a given swap quote
fn process_swap(quote: QuoteTx, witnesses: Vec<WitnessTx>) -> Vec<SendResult> {
    vec![]
}
