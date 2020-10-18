mod tests;

use crate::{
    common::{Coin, GenericCoinAmount, LokiAmount, PoolCoin, Timestamp},
    side_chain::SideChainTx,
    transactions::{OutputTx, PoolChangeTx, StakeQuoteTx, StakeTx, UnstakeRequestTx},
    vault::transactions::{
        memory_provider::{FulfilledTxWrapper, Portion, WitnessTxWrapper},
        TransactionProvider,
    },
};

use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use parking_lot::RwLock;
use uuid::Uuid;
use web3::types::U256;

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

pub(super) fn process_stakes<T: TransactionProvider>(tx_provider: &mut Arc<RwLock<T>>) {
    let provider = tx_provider.read();
    let stake_quote_txs = provider.get_stake_quote_txs();
    let witness_txs = provider.get_witness_txs();

    // TODO: a potential room for improvement: autoswap is relatively slow,
    // so we might want to release the mutex when performing it
    let new_txs = process_stakes_inner(&stake_quote_txs, &witness_txs);
    drop(provider);

    // TODO: make sure that things below happen atomically
    // (e.g. we don't want to send funds more than once if the
    // latest block info failed to have been updated)

    if let Err(err) = tx_provider.write().add_transactions(new_txs) {
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

/// Get the atomic amount corresponding to `portion` of `total`
fn get_portion_of_amount(total: u128, portion: Portion) -> u128 {
    let total: U256 = total.into();
    let portion: U256 = portion.0.into();
    let res = total * portion / U256::from(Portion::MAX.0);
    res.try_into().expect("overflow")
}

fn get_amounts_unstakable<T: TransactionProvider>(
    tx_provider: &T,
    pool: PoolCoin,
    staker: &str,
) -> Result<(LokiAmount, GenericCoinAmount), String> {
    info!("Handling unstake tx for staker: {}", staker);

    let portions = tx_provider.get_portions();

    let coin_portions = portions
        .get(&pool)
        .ok_or(format!("No portions for coin: {}", pool))?;

    let staker_portions = coin_portions.get(staker).ok_or(format!(
        "No portions for staker: {} in {} pool",
        staker, pool
    ))?;

    debug!("Staker portions: {:?}", staker_portions);

    // For now we assume that we want to withdraw everything

    // TODO: check that the fees that should be payed to
    // liquidity providers (whatever they are) are part of "liquidity"
    let liquidity = tx_provider
        .get_liquidity(pool)
        .expect("liquidity should exist");

    let loki_amount = get_portion_of_amount(liquidity.loki_depth, *staker_portions);
    let other_amount = get_portion_of_amount(liquidity.depth, *staker_portions);

    let loki = LokiAmount::from_atomic(loki_amount);
    let other = GenericCoinAmount::from_atomic(pool.get_coin(), other_amount);

    Ok((loki, other))
}

fn prepare_output_txs(
    tx: &UnstakeRequestTx,
    loki_amount: LokiAmount,
    other_amount: GenericCoinAmount,
) -> Result<(OutputTx, OutputTx), &'static str> {
    let loki = OutputTx::new(
        Timestamp::now(),
        tx.id,
        vec![],
        vec![],
        Coin::LOKI,
        tx.loki_address.clone(),
        loki_amount.to_atomic(),
    )
    .map_err(|_| "could not construct Loki output")?;

    let other = OutputTx::new(
        Timestamp::now(),
        tx.id,
        vec![],
        vec![],
        tx.pool.into(),
        tx.other_address.clone(),
        other_amount.to_atomic(),
    )
    .map_err(|_| "could not construct Other output")?;

    Ok((loki, other))
}

fn process_unstake_tx<T: TransactionProvider>(
    tx_provider: &T,
    tx: &UnstakeRequestTx,
) -> Result<(OutputTx, OutputTx, PoolChangeTx), String> {
    let staker = &tx.staker_id;

    // Find out how much we can unstake
    // NOTE: we might want to remove unstake qoutes if we can't process them
    let (loki_amount, other_amount) = get_amounts_unstakable(tx_provider, tx.pool, staker)?;

    debug!(
        "Amounts unstakable by {} are: {} and {:?}",
        staker, loki_amount, other_amount
    );

    let (loki_tx, other_tx) = prepare_output_txs(tx, loki_amount, other_amount)?;

    let d_loki: i128 = loki_amount
        .to_atomic()
        .try_into()
        .map_err(|_| "Loki amount overflow")?;
    let d_other: i128 = other_amount
        .to_atomic()
        .try_into()
        .map_err(|_| "Other amount overflow")?;

    let pool_change_tx = PoolChangeTx::new(tx.pool, -d_loki, -d_other);

    Ok((loki_tx, other_tx, pool_change_tx))
}

pub(super) fn process_unstakes<T: TransactionProvider>(tx_provider: &mut T) {
    let unstake_txs = tx_provider.get_unstake_request_txs();

    let mut output_txs: Vec<SideChainTx> = Vec::with_capacity(unstake_txs.len() * 3);

    for tx in unstake_txs {
        match process_unstake_tx(tx_provider, tx) {
            Ok((output1, output2, pool_change)) => {
                output_txs.push(output1.into());
                output_txs.push(output2.into());
                output_txs.push(pool_change.into());
            }
            Err(err) => {
                warn!("Failed to process unstake request {}: {}", tx.id, err);
            }
        }
    }

    tx_provider
        .add_transactions(output_txs)
        .expect("Could not add transactions");
}
