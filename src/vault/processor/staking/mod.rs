#[cfg(test)]
mod tests;

use crate::{
    common::*,
    local_store::LocalEvent,
    vault::transactions::{
        memory_provider::{FulfilledWrapper, Portion, StatusWitnessWrapper},
        TransactionProvider,
    },
};
use chainflip_common::types::{chain::*, coin::Coin, unique_id::GetUniqueId, Network};
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
    let witnesses = provider.get_witnesses();

    // TODO: a potential room for improvement: autoswap is relatively slow,
    // so we might want to release the mutex when performing it
    let new_events = process_deposit_quotes_inner(&deposit_quotes, &witnesses, network);
    drop(provider);

    // TODO: make sure that things below happen atomically
    // (e.g. we don't want to send funds more than once if the
    // latest block info failed to have been updated)

    if let Err(err) = tx_provider.write().add_local_events(new_events) {
        error!("Error adding a pool change tx: {}", err);
        panic!();
    };
}

/// Try to match witnesses with deposit quotes and return a list of deposits that should be added to the side chain
fn process_deposit_quotes_inner(
    quotes: &[FulfilledWrapper<DepositQuote>],
    witnesses: &[StatusWitnessWrapper],
    network: Network,
) -> Vec<LocalEvent> {
    let mut new_events = Vec::<LocalEvent>::default();

    for quote_info in quotes {
        // only process confirmed witnesses
        let wtxs: Vec<&StatusWitnessWrapper> = witnesses
            .iter()
            .filter(|wtx| wtx.is_confirmed() && wtx.inner.quote == quote_info.inner.unique_id())
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
                    quote_info.inner.unique_id()
                );
                new_events.extend(refunds.into_iter().map(|tx| tx.into()));
            }
        } else if let Some(res) = process_deposit_quote(quote_info, &wtxs, network) {
            new_events.reserve(new_events.len() + 2);
            // IMPORTANT: deposits should come before pool changes,
            // due to the way Transaction provider processes them
            new_events.push(res.deposit.into());
            new_events.push(res.pool_change.into());
        };
    }

    new_events
}

fn refund_deposit_quotes(
    quote_info: &FulfilledWrapper<DepositQuote>,
    witnesses: &[&StatusWitnessWrapper],
    network: Network,
) -> Vec<Output> {
    if !quote_info.fulfilled {
        return vec![];
    }

    let quote = &quote_info.inner;
    let quote_coin = quote.pool;
    let mut output_txs: Vec<Output> = vec![];

    let confirmed_witnesses = witnesses.iter().filter(|tx| tx.is_confirmed());

    for wtx in confirmed_witnesses {
        let tx = &wtx.inner;
        let return_address = match tx.coin {
            Coin::OXEN => quote.base_return_address.clone(),
            coin if coin == quote_coin => quote.coin_return_address.clone(),
            coin => {
                panic!(
                    "Found a witness for coin {} but quote is for coin {}",
                    coin, quote_coin
                );
            }
        };

        if tx.amount == 0 {
            warn!("Witness {} has amount 0", tx.unique_id());
            continue;
        }

        let output = Output {
            parent: OutputParent::DepositQuote(quote.unique_id()),
            witnesses: vec![tx.unique_id()],
            pool_changes: vec![],
            coin: tx.coin,
            address: return_address,
            amount: tx.amount,
            event_number: None,
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
    witnesses: &[&StatusWitnessWrapper],
    network: Network,
) -> Option<DepositQuoteResult> {
    if quote_info.fulfilled {
        return None;
    }

    debug!(
        "Found witness matching quote: {}",
        quote_info.inner.unique_id()
    );

    let quote = &quote_info.inner;

    let mut oxen_amount: Option<i128> = None;
    let mut other_amount: Option<i128> = None;

    // Indexes of used witnesses
    let mut wtx_idxs = Vec::<UniqueId>::default();

    for wtx in witnesses {
        // We don't expect processed or unconfirmed quotes quotes at this stage,
        // but let's double check this:
        if !wtx.is_confirmed() {
            continue;
        }

        let wtx = &wtx.inner;

        match wtx.coin {
            Coin::OXEN => {
                if oxen_amount.is_some() {
                    error!("Unexpected second oxen witness");
                    return None;
                }

                let amount = match i128::try_from(wtx.amount) {
                    Ok(amount) => amount,
                    Err(err) => {
                        error!("Invalid amount in quote: {} ({})", wtx.amount, err);
                        return None;
                    }
                };

                wtx_idxs.push(wtx.unique_id());
                oxen_amount = Some(amount);
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
                    wtx_idxs.push(wtx.unique_id());
                    other_amount = Some(amount);
                } else {
                    error!("Unexpected coin type: {}", coin_type);
                    return None;
                }
            }
        }
    }

    if oxen_amount.is_none() {
        debug!(
            "Oxen is not yet provisioned in quote: {}",
            quote.unique_id()
        );
    }

    if other_amount.is_none() {
        debug!(
            "{} is not yet provisioned in quote: {}",
            quote.pool,
            quote.unique_id()
        );
    }

    match (oxen_amount, other_amount) {
        (Some(oxen_amount), Some(other_amount)) => {
            let pool_change_tx = PoolChange::new(quote.pool, other_amount, oxen_amount, None);

            // TODO: autoswap goes here
            let oxen_amount: u128 = oxen_amount.try_into().expect("negative deposit");
            let other_amount: u128 = other_amount.try_into().expect("negative deposit");

            let deposit = Deposit {
                quote: quote.unique_id(),
                witnesses: wtx_idxs,
                pool_change: pool_change_tx.unique_id(),
                staker_id: quote.staker_id.clone(),
                pool: quote.pool,
                base_amount: oxen_amount,
                other_amount,
                event_number: None,
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
) -> Result<(OxenAmount, GenericCoinAmount), String> {
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

    let oxen_amount = get_portion_of_amount(liquidity.base_depth, *staker_portions);
    let other_amount = get_portion_of_amount(liquidity.depth, *staker_portions);

    let oxen = OxenAmount::from_atomic(oxen_amount);
    let other = GenericCoinAmount::from_atomic(pool.get_coin(), other_amount);

    Ok((oxen, other))
}

fn prepare_outputs(
    tx: &WithdrawRequest,
    oxen_amount: OxenAmount,
    other_amount: GenericCoinAmount,
    network: Network,
) -> Result<(Output, Output), &'static str> {
    let oxen = Output {
        parent: OutputParent::WithdrawRequest(tx.unique_id()),
        witnesses: vec![],
        pool_changes: vec![],
        coin: Coin::OXEN,
        address: tx.base_address.clone(),
        amount: oxen_amount.to_atomic(),
        event_number: None,
    };

    oxen.validate(network)
        .map_err(|_| "could not construct Oxen output")?;

    let other = Output {
        parent: OutputParent::WithdrawRequest(tx.unique_id()),
        witnesses: vec![],
        pool_changes: vec![],
        coin: tx.pool,
        address: tx.other_address.clone(),
        amount: other_amount.to_atomic(),
        event_number: None,
    };

    other
        .validate(network)
        .map_err(|_| "could not construct Oxen output")?;

    Ok((oxen, other))
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
    let (oxen_amount, other_amount) =
        get_amounts_withdrawable(tx_provider, PoolCoin::from(tx.pool).unwrap(), &staker)?;

    debug!(
        "Amounts withdrawable by {} are: {} and {:?}",
        staker, oxen_amount, other_amount
    );

    let (oxen_tx, other_tx) = prepare_outputs(tx, oxen_amount, other_amount, network)?;

    let d_oxen: i128 = oxen_amount
        .to_atomic()
        .try_into()
        .map_err(|_| "Oxen amount overflow")?;
    let d_other: i128 = other_amount
        .to_atomic()
        .try_into()
        .map_err(|_| "Other amount overflow")?;

    let pool_change_tx = PoolChange::new(tx.pool, -d_other, -d_oxen, None);
    pool_change_tx.validate(network)?;

    let withdraw = Withdraw {
        withdraw_request: tx.unique_id(),
        outputs: [oxen_tx.unique_id(), other_tx.unique_id()],
        event_number: None,
    };
    withdraw.validate(network)?;

    Ok((oxen_tx, other_tx, pool_change_tx, withdraw))
}

pub(super) fn process_withdraw_requests<T: TransactionProvider>(
    tx_provider: &mut T,
    network: Network,
) {
    let withdraw_request_events = tx_provider.get_withdraw_requests();

    let (valid_evts, invalid_evts): (Vec<_>, Vec<_>) = withdraw_request_events
        .iter()
        .filter(|tx| !tx.fulfilled)
        .partition(|tx| tx.inner.verify_signature());

    for tx in invalid_evts {
        warn!(
            "Invalid signature for withdraw request {}",
            tx.inner.unique_id()
        );
    }

    // TODO: We shouldn't be getting invalid signatures as we already validate
    // them before adding to the database, but since we check them again, we
    // we should handle the case where they are invalid (by removing from the db)

    let mut new_events: Vec<LocalEvent> = Vec::with_capacity(valid_evts.len() * 4);

    for tx in valid_evts {
        match process_withdraw_request(tx_provider, tx, network) {
            Ok((output1, output2, pool_change, withdraw)) => {
                new_events.push(output1.into());
                new_events.push(output2.into());
                new_events.push(pool_change.into());
                new_events.push(withdraw.into());
            }
            Err(err) => {
                warn!(
                    "Failed to process withdraw request {}: {}",
                    tx.inner.unique_id(),
                    err
                );
            }
        }
    }

    tx_provider
        .add_local_events(new_events)
        .expect("Could not add transactions");
}
