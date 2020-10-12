mod tests;

use crate::{
    common::{Coin, LokiAmount},
    side_chain::SideChainTx,
    transactions::{PoolChangeTx, StakeQuoteTx, StakeTx},
    vault::transactions::{
        memory_provider::{FulfilledTxWrapper, WitnessTxWrapper},
        TransactionProvider,
    },
};

use std::convert::{TryFrom, TryInto};

use uuid::Uuid;

/// A set of transaction to be added to the side chain as a result
/// of a successful match between stake and witness transactions
struct StakeQuoteResult {
    stake_tx: StakeTx,
    pool_change: PoolChangeTx,
}

impl StakeQuoteResult {
    pub fn new(stake_tx: StakeTx, pool_change: PoolChangeTx) -> Self {
        StakeQuoteResult {
            stake_tx,
            pool_change,
        }
    }
}

pub(super) fn process_stakes<T: TransactionProvider>(tx_provider: &mut T) {
    let stake_quote_txs = tx_provider.get_stake_quote_txs();
    let witness_txs = tx_provider.get_witness_txs();

    let new_txs = process_stakes_inner(stake_quote_txs, witness_txs);

    // TODO: make sure that things below happen atomically
    // (e.g. we don't want to send funds more than once if the
    // latest block info failed to have been updated)

    if let Err(err) = tx_provider.add_transactions(new_txs) {
        error!("Error adding a pool change tx: {}", err);
        panic!();
    };
}

/// Try to match witness transacitons with stake transactions and return a list of
/// transactions that should be added to the side chain
fn process_stakes_inner(
    quotes: &[FulfilledTxWrapper<StakeQuoteTx>],
    witness_txs: &[WitnessTxWrapper],
) -> Vec<SideChainTx> {
    let mut new_txs = Vec::<SideChainTx>::default();

    for quote_info in quotes {
        // Find all relevant witness transactions
        let wtxs: Vec<&WitnessTxWrapper> = witness_txs
            .iter()
            .filter(|wtx| !wtx.used && wtx.inner.quote_id == quote_info.inner.id)
            .collect();

        if !wtxs.is_empty() {
            if let Some(res) = process_stake_quote(quote_info, &wtxs) {
                new_txs.reserve(new_txs.len() + 2);
                // IMPORTANT: stake transactions should come before pool change transactions,
                // due to the way Transaction provider processes them
                new_txs.push(res.stake_tx.into());
                new_txs.push(res.pool_change.into());
            }
        }
    }

    new_txs
}

/// Process a single stake quote with all witness transactions referencing it
fn process_stake_quote(
    quote_info: &FulfilledTxWrapper<StakeQuoteTx>,
    witness_txs: &[&WitnessTxWrapper],
) -> Option<StakeQuoteResult> {
    // TODO: put a balance change tx onto the side chain
    info!("Found witness matching quote: {:?}", quote_info.inner);

    // TODO: only print this if a witness is not used:

    // For now only process unfulfilled ones:
    if quote_info.fulfilled {
        warn!("Witness matches an already fulfilled quote. Should refund?");
        return None;
    }

    let quote = &quote_info.inner;

    let mut loki_amount: Option<i128> = None;
    let mut other_amount: Option<i128> = None;

    // Indexes of used witness transaction
    let mut wtx_idxs = Vec::<Uuid>::default();

    for wtx in witness_txs {
        // We don't expect used quotes at this stage,
        // but let's double check this:
        if wtx.used {
            continue;
        }

        let wtx = &wtx.inner;

        match wtx.coin {
            Coin::LOKI => {
                if loki_amount.is_some() {
                    error!("Unexpected second loki witness transaction");
                    return None;
                }

                let amount = match i128::try_from(wtx.amount) {
                    Ok(amount) => amount,
                    Err(err) => {
                        error!("Invalid amount in quote: {} ({})", wtx.amount, err);
                        return None;
                    }
                };

                wtx_idxs.push(wtx.id);
                loki_amount = Some(amount);
            }
            coin_type @ _ => {
                if coin_type == quote.coin_type.get_coin() {
                    if other_amount.is_some() {
                        error!("Unexpected second loki witness transaction");
                        return None;
                    }

                    let amount = match i128::try_from(wtx.amount) {
                        Ok(amount) => amount,
                        Err(err) => {
                            error!("Invalid amount in quote: {} ({})", wtx.amount, err);
                            return None;
                        }
                    };
                    wtx_idxs.push(wtx.id);
                    other_amount = Some(amount);
                } else {
                    error!("Unexpected coin type: {}", coin_type);
                    return None;
                }
            }
        }
    }

    if loki_amount.is_none() {
        info!("Loki is not yet provisioned in quote: {:?}", quote);
    }

    if other_amount.is_none() {
        info!(
            "{} is not yet provisioned in quote: {:?}",
            quote.coin_type.get_coin(),
            quote
        );
    }

    match (loki_amount, other_amount) {
        (Some(loki_amount), Some(other_amount)) => {
            let pool_coin = quote.coin_type;

            let pool_change_tx = PoolChangeTx::new(pool_coin, loki_amount, other_amount);

            // TODO: autoswap goes here
            let loki_amount: u128 = loki_amount.try_into().expect("negative stake");
            let loki_amount = LokiAmount::from_atomic(loki_amount);

            let other_amount: u128 = other_amount.try_into().expect("negative stake");

            let stake_tx = StakeTx {
                id: Uuid::new_v4(),
                pool_change_tx: pool_change_tx.id,
                quote_tx: quote.id,
                witness_txs: wtx_idxs,
                staker_id: quote.staker_id.clone(),
                pool: pool_coin,
                loki_amount,
                other_amount,
            };

            Some(StakeQuoteResult::new(stake_tx, pool_change_tx))
        }
        _ => None,
    }
}
