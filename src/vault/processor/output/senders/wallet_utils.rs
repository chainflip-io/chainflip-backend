use std::fmt::Display;

use crate::{transactions::OutputTx, utils::bip44::KeyPair};

/// A struct for representing wallet balance
#[derive(Debug, Clone)]
pub struct WalletBalance {
    wallet: KeyPair,
    balance: u128,
}

impl WalletBalance {
    pub fn new(wallet: KeyPair, balance: u128) -> Self {
        WalletBalance { wallet, balance }
    }
}

impl Display for WalletBalance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.wallet.public_key, self.balance)
    }
}

/// An output mapped to a wallet
pub struct WalletOutput {
    /// The wallet
    pub wallet: KeyPair,
    /// The output to send
    pub output: OutputTx,
}

/// Get the sending wallets for the given outputs
///
/// This uses a very basic greedy algorithm at the moment
pub fn get_sending_wallets(balances: &[WalletBalance], outputs: &[OutputTx]) -> Vec<WalletOutput> {
    if balances.is_empty() {
        warn!("Empty wallet balances passed to get_sending_wallets");
        return vec![];
    }

    let mut wallet_outputs = vec![];
    let mut balances = balances.to_vec();

    // Sort output amounts by biggest to smallets
    let mut outputs = Vec::from(outputs);
    outputs.sort_by(|a, b| b.amount.cmp(&a.amount));

    for output in outputs {
        // Sort balances from biggest to smallest
        balances.sort_by(|a, b| b.balance.cmp(&a.balance));

        // Get the first balance and see if we can fit the output into it. If we can't then we'll have to skip this output :(
        let wallet_balance = balances.first_mut().unwrap();
        if wallet_balance.balance >= output.amount {
            match wallet_balance.balance.checked_sub(output.amount) {
                Some(new_balance) => {
                    let wallet_output = WalletOutput {
                        wallet: wallet_balance.wallet.clone(),
                        output: output.clone(),
                    };

                    wallet_outputs.push(wallet_output);
                    wallet_balance.balance = new_balance;
                }
                None => warn!(
                    "Error occured while subtracting {} from wallet balance {}",
                    output.amount, wallet_balance.balance
                ),
            };
        } else {
            warn!(
                "Cannot find a suitable wallet for Output: {}, balance: {}",
                output.id, output.amount
            );
        }
    }

    wallet_outputs
}

#[cfg(test)]
mod test {
    use rand::{thread_rng, Rng};

    use crate::{common::Coin, utils::test_utils::create_fake_output_tx};

    use super::*;

    fn get_key_pair() -> KeyPair {
        let random_bytes = thread_rng().gen::<[u8; 32]>();
        let private_key = hex::encode(random_bytes);
        KeyPair::from_private_key(&private_key).unwrap()
    }

    #[test]
    fn returns_empty_if_no_wallet_balances() {
        assert!(get_sending_wallets(&vec![], &vec![]).is_empty());
    }

    #[test]
    fn returns_wallet_outputs() {
        let mut biggest_output_tx = create_fake_output_tx(Coin::LOKI);
        biggest_output_tx.amount = 1000;
        let mut big_output_tx = create_fake_output_tx(Coin::LOKI);
        big_output_tx.amount = 750;
        let mut medium_output_tx = create_fake_output_tx(Coin::LOKI);
        medium_output_tx.amount = 500;
        let mut small_output_tx = create_fake_output_tx(Coin::LOKI);
        small_output_tx.amount = 100;

        let outputs = vec![
            big_output_tx.clone(),
            small_output_tx,
            medium_output_tx.clone(),
            biggest_output_tx.clone(),
        ];

        let key_1 = get_key_pair();
        let key_2 = get_key_pair();

        let balances = vec![
            WalletBalance {
                wallet: key_1.clone(),
                balance: 1750,
            },
            WalletBalance {
                wallet: key_2.clone(),
                balance: 550,
            },
        ];

        let outputs = get_sending_wallets(&balances, &outputs);
        assert_eq!(outputs.len(), 3);

        let first = outputs.get(0).unwrap();
        assert_eq!(first.output, biggest_output_tx);
        assert_eq!(&first.wallet, &key_1);

        let second = outputs.get(1).unwrap();
        assert_eq!(second.output, big_output_tx);
        assert_eq!(&second.wallet, &key_1);

        let third = outputs.get(2).unwrap();
        assert_eq!(third.output, medium_output_tx);
        assert_eq!(&third.wallet, &key_2);
    }
}
