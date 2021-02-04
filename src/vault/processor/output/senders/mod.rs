use crate::vault::transactions::TransactionProvider;
use chainflip_common::types::{
    chain::{Output, OutputSent},
    coin::Coin,
};
use itertools::Itertools;
use parking_lot::RwLock;
use std::{convert::TryInto, sync::Arc};

pub mod btc;
pub mod ethereum;
pub mod loki_sender;
pub(super) mod wallet_utils;

/// A trait for an output sender
#[async_trait]
pub trait OutputSender {
    /// Send the given outputs and return output sent txs
    async fn send(&self, outputs: &[Output]) -> Vec<OutputSent>;
}

/// Groups outputs where the total amount is less than u128::MAX
fn group_outputs_by_sending_amounts<'a>(outputs: &'a [Output]) -> Vec<(u128, Vec<&'a Output>)> {
    let mut groups: Vec<(u128, Vec<&Output>)> = vec![];
    if outputs.is_empty() {
        return vec![];
    }

    let mut current_amount: u128 = 0;
    let mut current_outputs: Vec<&Output> = vec![];
    for output in outputs {
        match current_amount.checked_add(output.amount) {
            Some(amount) => {
                current_amount = amount;
                current_outputs.push(output);
            }
            None => {
                let outputs = current_outputs;
                groups.push((current_amount, outputs));
                current_amount = output.amount;
                current_outputs = vec![output];
            }
        }
    }

    groups.push((current_amount, current_outputs));

    groups
}

/// Get input id indices
pub fn get_input_id_indices<T: TransactionProvider>(
    provider: Arc<RwLock<T>>,
    coin: Coin,
) -> Vec<u32> {
    if coin == Coin::LOKI {
        return vec![];
    }

    let provider = provider.read();

    let swaps = provider
        .get_swap_quotes()
        .iter()
        .filter_map(|quote| {
            let quote = &quote.inner;
            if quote.input == coin {
                if let Ok(bytes) = quote.input_address_id.clone().try_into() {
                    return Some(u32::from_be_bytes(bytes));
                }
            }

            None
        })
        .collect_vec();

    let deposit_quotes = provider
        .get_deposit_quotes()
        .iter()
        .filter_map(|quote| {
            let quote = &quote.inner;
            if quote.pool == coin {
                if let Ok(bytes) = quote.coin_input_address_id.clone().try_into() {
                    return Some(u32::from_be_bytes(bytes));
                }
            }

            None
        })
        .collect_vec();

    [vec![0], swaps, deposit_quotes].concat()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        local_store::MemoryLocalStore, utils::test_utils::data::TestData,
        vault::transactions::MemoryTransactionsProvider,
    };
    use std::sync::Mutex;

    #[test]
    fn test_group_outputs_by_sending_amounts() {
        let mut eth_output_1 = TestData::output(Coin::ETH, 10);
        let mut eth_output_2 = TestData::output(Coin::ETH, 10);

        eth_output_1.amount = 100;
        eth_output_2.amount = 200;

        let vec = vec![eth_output_1.clone(), eth_output_2.clone()];
        let result = group_outputs_by_sending_amounts(&vec);

        assert_eq!(result.len(), 1);
        assert_eq!(result, vec![(300, vec![&eth_output_1, &eth_output_2])]);

        // Split amounts into 2

        eth_output_1.amount = u128::MAX;
        eth_output_2.amount = 300;

        let vec = vec![eth_output_1.clone(), eth_output_2.clone()];
        let result = group_outputs_by_sending_amounts(&vec);

        assert_eq!(result.len(), 2);
        assert_eq!(
            result,
            vec![(u128::MAX, vec![&eth_output_1]), (300, vec![&eth_output_2])]
        );

        // Max of u128

        eth_output_1.amount = (u128::MAX / 2) + 1; // Ensure we get u128::MAX when adding 2 values because dividing by 2 will round down
        eth_output_2.amount = u128::MAX / 2;

        let vec = vec![eth_output_1.clone(), eth_output_2.clone()];
        let result = group_outputs_by_sending_amounts(&vec);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result,
            vec![(u128::MAX, vec![&eth_output_1, &eth_output_2])]
        );
    }

    #[test]
    fn test_get_input_id_indices() {
        let local_store = MemoryLocalStore::new();
        let local_store = Arc::new(Mutex::new(local_store));
        let provider = MemoryTransactionsProvider::new_protected(local_store.clone());

        let mut eth_quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        eth_quote.input_address_id = 5u32.to_be_bytes().to_vec();

        let mut btc_quote = TestData::swap_quote(Coin::BTC, Coin::LOKI);
        btc_quote.input_address_id = 6u32.to_be_bytes().to_vec();

        let mut eth_deposit = TestData::deposit_quote(Coin::ETH);
        eth_deposit.coin_input_address_id = 7u32.to_be_bytes().to_vec();

        let mut btc_deposit = TestData::deposit_quote(Coin::BTC);
        btc_deposit.coin_input_address_id = 8u32.to_be_bytes().to_vec();

        provider
            .write()
            .add_local_events(vec![
                eth_quote.into(),
                btc_quote.into(),
                eth_deposit.into(),
                btc_deposit.into(),
            ])
            .unwrap();

        let indices = get_input_id_indices(provider.clone(), Coin::ETH);
        assert_eq!(&indices, &[0, 5, 7]);

        let indices = get_input_id_indices(provider.clone(), Coin::BTC);
        assert_eq!(&indices, &[0, 6, 8]);

        let indices = get_input_id_indices(provider.clone(), Coin::LOKI);
        assert!(indices.is_empty());
    }
}
