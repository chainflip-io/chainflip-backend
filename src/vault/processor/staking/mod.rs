#[cfg(test)]
mod tests;

use crate::{
    common::*,
    side_chain::SideChainTx,
    vault::transactions::{
        memory_provider::{FulfilledWrapper, Portion, UsedWitnessWrapper},
        TransactionProvider,
    },
};
use chainflip_common::types::{chain::*, coin::Coin, Network, Timestamp, UUIDv4};
use parking_lot::RwLock;
use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
};
use web3::types::U256;

/// A set of transaction to be added to the side chain as a result
/// of a successful match between deposit and witnesses
struct DepositQuoteResult {
    deposit: Deposit,
    pool_change: PoolChange,
}

impl DepositQuoteResult {
    pub fn new(deposit: Deposit, pool_change: PoolChange) -> Self {
        DepositQuoteResult {
            deposit: deposit,
            pool_change,
        }
    }
}

pub(super) fn process_deposit_quotes<T: TransactionProvider>(
    tx_provider: &mut Arc<RwLock<T>>,
    network: Network,
) {
    let provider = tx_provider.read();
    let deposit_quotes = provider.get_deposit_quotes();
    let witness_txs = provider.get_witnesses();

    // TODO: a potential room for improvement: autoswap is relatively slow,
    // so we might want to release the mutex when performing it
    let new_txs = process_deposit_quotes_inner(&deposit_quotes, &witness_txs, network);
    drop(provider);

    // TODO: make sure that things below happen atomically
    // (e.g. we don't want to send funds more than once if the
    // latest block info failed to have been updated)

    if let Err(err) = tx_provider.write().add_transactions(new_txs) {
        error!("Error adding a pool change tx: {}", err);
        panic!();
    };
}

/// Try to match witnesses with deposit quotes and return a list of deposits that should be added to the side chain
fn process_deposit_quotes_inner(
    quotes: &[FulfilledWrapper<DepositQuote>],
    witness_txs: &[UsedWitnessWrapper],
    network: Network,
) -> Vec<SideChainTx> {
    let mut new_txs = Vec::<SideChainTx>::default();

    for quote_info in quotes {
        // Find all relevant witnesses
        let wtxs: Vec<&UsedWitnessWrapper> = witness_txs
            .iter()
            .filter(|wtx| !wtx.used && wtx.inner.quote == quote_info.inner.id)
            .collect();

        if wtxs.is_empty() {
            continue;
        }

        // Refund the user if the quote is fulfilled
        if quote_info.fulfilled {
            let refunds = refund_deposit_quotes(quote_info, &wtxs, network);
            if refunds.len() > 0 {
                info!(
                    "Quote {} is already fulfilled, refunding!",
                    quote_info.inner.id
                );
                new_txs.extend(refunds.into_iter().map(|tx| tx.into()));
            }
        } else if let Some(res) = process_deposit_quote(quote_info, &wtxs, network) {
            new_txs.reserve(new_txs.len() + 2);
            // IMPORTANT: deposits should come before pool changes,
            // due to the way Transaction provider processes them
            new_txs.push(res.deposit.into());
            new_txs.push(res.pool_change.into());
        };
    }

    new_txs
}

fn refund_deposit_quotes(
    quote_info: &FulfilledWrapper<DepositQuote>,
    witness_txs: &[&UsedWitnessWrapper],
    network: Network,
) -> Vec<Output> {
    if !quote_info.fulfilled {
        return vec![];
    }

    let quote = &quote_info.inner;
    let quote_coin = quote.pool;
    let mut output_txs: Vec<Output> = vec![];

    let valid_witness_txs = witness_txs.iter().filter(|tx| !tx.used);

    for wtx in valid_witness_txs {
        let tx = &wtx.inner;
        let return_address = match tx.coin {
            Coin::LOKI => quote.base_return_address.clone(),
            coin if coin == quote_coin => quote.coin_return_address.clone(),
            coin => {
                panic!(
                    "Found a witness for coin {} but quote is for coin {}",
                    coin, quote_coin
                );
            }
        };

        if tx.amount == 0 {
            warn!("Witness {} has amount 0", tx.id);
            continue;
        }

        let output = Output {
            id: UUIDv4::new(),
            timestamp: Timestamp::now(),
            parent: OutputParent::DepositQuote(quote.id),
            witnesses: vec![tx.id],
            pool_changes: vec![],
            coin: tx.coin,
            address: return_address,
            amount: tx.amount,
        };

        match output.validate(network) {
            Ok(_) => output_txs.push(output),
            Err(err) => warn!(
                "Failed to create refund output for deposit witness {:?}: {}",
                tx, err
            ),
        }
    }

    output_txs
}

/// Process a single deposit quote with all witnesses referencing it
fn process_deposit_quote(
    quote_info: &FulfilledWrapper<DepositQuote>,
    witness_txs: &[&UsedWitnessWrapper],
    network: Network,
) -> Option<DepositQuoteResult> {
    if quote_info.fulfilled {
        return None;
    }

    debug!("Found witness matching quote: {}", quote_info.inner.id);

    let quote = &quote_info.inner;

    let mut loki_amount: Option<i128> = None;
    let mut other_amount: Option<i128> = None;

    // Indexes of used witnesses
    let mut wtx_idxs = Vec::<UUIDv4>::default();

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
                    error!("Unexpected second loki witness");
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
                if coin_type == quote.pool {
                    if other_amount.is_some() {
                        error!("Unexpected second {} witness", coin_type);
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
        debug!("Loki is not yet provisioned in quote: {}", quote.id);
    }

    if other_amount.is_none() {
        debug!(
            "{} is not yet provisioned in quote: {}",
            quote.pool, quote.id
        );
    }

    match (loki_amount, other_amount) {
        (Some(loki_amount), Some(other_amount)) => {
            let pool_change_tx = PoolChange {
                id: UUIDv4::new(),
                timestamp: Timestamp::now(),
                pool: quote.pool,
                depth_change: other_amount,
                base_depth_change: loki_amount,
            };

            // TODO: autoswap goes here
            let loki_amount: u128 = loki_amount.try_into().expect("negative deposit");
            let other_amount: u128 = other_amount.try_into().expect("negative deposit");

            let deposit = Deposit {
                id: UUIDv4::new(),
                timestamp: Timestamp::now(),
                quote: quote.id,
                witnesses: wtx_idxs,
                pool_change: pool_change_tx.id,
                staker_id: quote.staker_id.clone(),
                pool: quote.pool,
                base_amount: loki_amount,
                other_amount,
            };

            match deposit.validate(network) {
                Ok(_) => Some(DepositQuoteResult::new(deposit, pool_change_tx)),
                Err(_) => {
                    warn!("Invalid deposit {:?}", deposit);
                    None
                }
            }
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

fn get_amounts_withdrawable<T: TransactionProvider>(
    tx_provider: &T,
    pool: PoolCoin,
    staker: &StakerId,
) -> Result<(LokiAmount, GenericCoinAmount), String> {
    info!("Handling withdraw tx for staker: {}", staker);

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

    let loki_amount = get_portion_of_amount(liquidity.base_depth, *staker_portions);
    let other_amount = get_portion_of_amount(liquidity.depth, *staker_portions);

    let loki = LokiAmount::from_atomic(loki_amount);
    let other = GenericCoinAmount::from_atomic(pool.get_coin(), other_amount);

    Ok((loki, other))
}

fn prepare_outputs(
    tx: &WithdrawRequest,
    loki_amount: LokiAmount,
    other_amount: GenericCoinAmount,
    network: Network,
) -> Result<(Output, Output), &'static str> {
    let loki = Output {
        id: UUIDv4::new(),
        timestamp: Timestamp::now(),
        parent: OutputParent::WithdrawRequest(tx.id),
        witnesses: vec![],
        pool_changes: vec![],
        coin: Coin::LOKI,
        address: tx.base_address.clone(),
        amount: loki_amount.to_atomic(),
    };

    loki.validate(network)
        .map_err(|_| "could not construct Loki output")?;

    let other = Output {
        id: UUIDv4::new(),
        timestamp: Timestamp::now(),
        parent: OutputParent::WithdrawRequest(tx.id),
        witnesses: vec![],
        pool_changes: vec![],
        coin: tx.pool,
        address: tx.other_address.clone(),
        amount: other_amount.to_atomic(),
    };

    other
        .validate(network)
        .map_err(|_| "could not construct Loki output")?;

    Ok((loki, other))
}

fn process_withdraw_request<T: TransactionProvider>(
    tx_provider: &T,
    tx: &FulfilledWrapper<WithdrawRequest>,
    network: Network,
) -> Result<(Output, Output, PoolChange, Withdraw), String> {
    let tx = &tx.inner;

    let staker = StakerId::from_bytes(&tx.staker_id).unwrap();

    // Find out how much we can withdraw
    // NOTE: we might want to remove withdraw requests if we can't process them
    let (loki_amount, other_amount) =
        get_amounts_withdrawable(tx_provider, PoolCoin::from(tx.pool).unwrap(), &staker)?;

    debug!(
        "Amounts withdrawable by {} are: {} and {:?}",
        staker, loki_amount, other_amount
    );

    let (loki_tx, other_tx) = prepare_outputs(tx, loki_amount, other_amount, network)?;

    let d_loki: i128 = loki_amount
        .to_atomic()
        .try_into()
        .map_err(|_| "Loki amount overflow")?;
    let d_other: i128 = other_amount
        .to_atomic()
        .try_into()
        .map_err(|_| "Other amount overflow")?;

    let pool_change_tx = PoolChange {
        id: UUIDv4::new(),
        timestamp: Timestamp::now(),
        pool: tx.pool,
        depth_change: -d_other,
        base_depth_change: -d_loki,
    };
    pool_change_tx.validate(network)?;

    let withdraw = Withdraw {
        id: UUIDv4::new(),
        timestamp: Timestamp::now(),
        withdraw_request: tx.id,
        outputs: [loki_tx.id, other_tx.id],
    };
    withdraw.validate(network)?;

    Ok((loki_tx, other_tx, pool_change_tx, withdraw))
}

pub(super) fn process_withdraw_requests<T: TransactionProvider>(
    tx_provider: &mut T,
    network: Network,
) {
    let withdraw_request_txs = tx_provider.get_withdraw_requests();

    let (valid_txs, invalid_txs): (Vec<_>, Vec<_>) = withdraw_request_txs
        .iter()
        .filter(|tx| !tx.fulfilled)
        .partition(|tx| tx.inner.verify_signature());

    for tx in invalid_txs {
        warn!("Invalid signature for withdraw request {}", tx.inner.id);
    }

    // TODO: We shouldn't be getting invalid signatures as we already validate
    // them before adding to the database, but since we check them again, we
    // we should handle the case where they are invalid (by removing from the db)

    let mut new_txs: Vec<SideChainTx> = Vec::with_capacity(valid_txs.len() * 4);

    for tx in valid_txs {
        match process_withdraw_request(tx_provider, tx, network) {
            Ok((output1, output2, pool_change, withdraw)) => {
                new_txs.push(output1.into());
                new_txs.push(output2.into());
                new_txs.push(pool_change.into());
                new_txs.push(withdraw.into());
            }
            Err(err) => {
                warn!(
                    "Failed to process withdraw request {}: {}",
                    tx.inner.id, err
                );
            }
        }
    }

    tx_provider
        .add_transactions(new_txs)
        .expect("Could not add transactions");
}
