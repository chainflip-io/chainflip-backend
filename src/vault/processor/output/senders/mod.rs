use crate::{
    common::Coin,
    transactions::{OutputSentTx, OutputTx},
};
use async_trait::async_trait;
use std::collections::HashMap;
use uuid::Uuid;

/// A trait for an output sender
#[async_trait]
pub trait OutputSender {
    /// Send the given outputs and return output sent txs
    async fn send(&self, outputs: &[OutputTx]) -> Vec<OutputSentTx>;
}

fn group_outputs_by_quote(outputs: &[OutputTx], coin_type: Coin) -> Vec<(Uuid, Vec<OutputTx>)> {
    // Make sure we only have valid outputs and group them by the quote
    let valid_txs = outputs.iter().filter(|tx| tx.coin == coin_type);

    let mut map: HashMap<Uuid, Vec<OutputTx>> = HashMap::new();
    for tx in valid_txs {
        let entry = map.entry(tx.quote_tx).or_insert(vec![]);
        entry.push(tx.clone());
    }

    map.into_iter()
        .map(|(quote, outputs)| (quote, outputs))
        .collect()
}

/// Groups outputs where the total amount is less than u128::MAX
fn group_outputs_by_sending_amounts<'a>(outputs: &'a [OutputTx]) -> Vec<(u128, Vec<&'a OutputTx>)> {
    let mut groups: Vec<(u128, Vec<&OutputTx>)> = vec![];
    if outputs.is_empty() {
        return vec![];
    }

    let mut current_amount: u128 = 0;
    let mut current_outputs: Vec<&OutputTx> = vec![];
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

pub mod btc;
pub mod ethereum;
pub mod loki_sender;

#[cfg(test)]
mod test {

    use super::*;
    use crate::utils::test_utils::create_fake_output_tx;

    #[test]
    fn test_group_outputs_by_quote() {
        let loki_output = create_fake_output_tx(Coin::LOKI);
        let mut btc_output_1 = create_fake_output_tx(Coin::BTC);
        let mut btc_output_2 = create_fake_output_tx(Coin::BTC);
        let mut btc_output_3 = create_fake_output_tx(Coin::BTC);
        let mut btc_output_4 = create_fake_output_tx(Coin::BTC);

        let quote_1 = uuid::Uuid::new_v4();
        btc_output_1.quote_tx = quote_1;
        btc_output_3.quote_tx = quote_1;

        let quote_2 = uuid::Uuid::new_v4();
        btc_output_2.quote_tx = quote_2;
        btc_output_4.quote_tx = quote_2;

        let result = group_outputs_by_quote(
            &[
                loki_output,
                btc_output_1.clone(),
                btc_output_2.clone(),
                btc_output_3.clone(),
                btc_output_4.clone(),
            ],
            Coin::BTC,
        );

        assert_eq!(result.len(), 2);

        let first = result.iter().find(|(quote, _)| quote == &quote_1).unwrap();
        assert_eq!(first.0, quote_1);
        assert_eq!(first.1, vec![btc_output_1, btc_output_3]);

        let second = result.iter().find(|(quote, _)| quote == &quote_2).unwrap();
        assert_eq!(second.0, quote_2);
        assert_eq!(second.1, vec![btc_output_2, btc_output_4]);
    }

    #[test]
    fn test_group_outputs_by_sending_amounts() {
        let mut eth_output_1 = create_fake_output_tx(Coin::ETH);
        let mut eth_output_2 = create_fake_output_tx(Coin::ETH);

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
}
