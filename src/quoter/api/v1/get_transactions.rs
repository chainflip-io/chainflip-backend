use crate::{
    common::api::ResponseError, common::StakerId, quoter::StateProvider, side_chain::SideChainTx,
    transactions::OutputTx,
};
use reqwest::StatusCode;
use serde::Deserialize;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

/// Parameters for GET `transactions` endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsParams {
    /// The quote id
    pub quote_id: Option<String>,
    /// The staker id
    pub staker_id: Option<String>,
}

/// Get all the transactions related to a quote
///
/// # Example Queries
///
/// > GET /v1/transactions?quoteId=xyz
///
/// > GET /v1/transactions?stakerId=xyz
pub async fn get_transactions<S>(
    params: TransactionsParams,
    state: Arc<Mutex<S>>,
) -> Result<Vec<SideChainTx>, ResponseError>
where
    S: StateProvider,
{
    if let Some(quote_id) = params.quote_id {
        let id = match Uuid::from_str(&quote_id) {
            Ok(id) => id,
            Err(_) => {
                return Err(ResponseError::new(
                    StatusCode::BAD_REQUEST,
                    "Invalid quote id",
                ))
            }
        };

        return get_quote_id_transactions(id, state);
    } else if let Some(staker_id) = params.staker_id {
        let id = match StakerId::new(&staker_id) {
            Ok(id) => id,
            Err(_) => {
                return Err(ResponseError::new(
                    StatusCode::BAD_REQUEST,
                    "Invalid staker id",
                ))
            }
        };
        return get_staker_id_transactions(id, state);
    }

    Ok(vec![])
}

fn get_quote_id_transactions<S>(
    id: Uuid,
    state: Arc<Mutex<S>>,
) -> Result<Vec<SideChainTx>, ResponseError>
where
    S: StateProvider,
{
    let state = state.lock().unwrap();

    let witnesses = state.get_witness_txs();
    let outputs = state.get_output_txs();
    let sent = state.get_output_sent_txs();
    let stakes = state.get_stake_txs();

    drop(state);

    // I know this is terribly inefficient but it'll have to do for now until we can clean it up :(

    let filtered_witnesses: Vec<SideChainTx> = witnesses
        .into_iter()
        .filter(|tx| tx.quote_id == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_stake: Vec<SideChainTx> = stakes
        .into_iter()
        .filter(|tx| tx.quote_tx == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_outputs: Vec<OutputTx> =
        outputs.into_iter().filter(|tx| tx.quote_tx == id).collect();
    let ids: Vec<Uuid> = filtered_outputs.iter().map(|tx| tx.id).collect();
    let filtered_outputs: Vec<SideChainTx> =
        filtered_outputs.into_iter().map(|tx| tx.into()).collect();

    let filtered_output_sent: Vec<SideChainTx> = sent
        .into_iter()
        .filter(|tx| ids.iter().find(|id| tx.output_txs.contains(id)).is_some())
        .map(|tx| tx.into())
        .collect();

    Ok([
        filtered_witnesses,
        filtered_stake,
        filtered_outputs,
        filtered_output_sent,
    ]
    .concat())
}

fn get_staker_id_transactions<S>(
    id: StakerId,
    state: Arc<Mutex<S>>,
) -> Result<Vec<SideChainTx>, ResponseError>
where
    S: StateProvider,
{
    let state = state.lock().unwrap();

    let quotes = state.get_stake_quotes();
    let witnesses = state.get_witness_txs();
    let outputs = state.get_output_txs();
    let sent = state.get_output_sent_txs();
    let stakes = state.get_stake_txs();
    let unstakes = state.get_unstake_txs();

    drop(state);

    let mut unstakes = unstakes.into_iter().filter(|tx| tx.staker_id == id);
    let mut quotes = quotes.into_iter().filter(|tx| tx.staker_id == id);

    let filtered_witnesses: Vec<SideChainTx> = witnesses
        .into_iter()
        .filter(|tx| quotes.find(|quote| tx.quote_id == quote.id).is_some())
        .map(|tx| tx.into())
        .collect();

    let filtered_outputs: Vec<OutputTx> = outputs
        .into_iter()
        .filter(|tx| unstakes.find(|quote| quote.id == tx.quote_tx).is_some())
        .collect();
    let ids: Vec<Uuid> = filtered_outputs.iter().map(|tx| tx.id).collect();
    let filtered_outputs: Vec<SideChainTx> =
        filtered_outputs.into_iter().map(|tx| tx.into()).collect();

    let filtered_output_sent: Vec<SideChainTx> = sent
        .into_iter()
        .filter(|tx| ids.iter().find(|id| tx.output_txs.contains(id)).is_some())
        .map(|tx| tx.into())
        .collect();

    let filtered_quotes: Vec<SideChainTx> = quotes.map(|tx| tx.into()).collect();

    let filtered_stakes: Vec<SideChainTx> = stakes
        .into_iter()
        .filter(|tx| tx.staker_id == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_unstakes: Vec<SideChainTx> = unstakes.map(|tx| tx.into()).collect();

    Ok([
        filtered_quotes,
        filtered_witnesses,
        filtered_stakes,
        filtered_unstakes,
        filtered_outputs,
        filtered_output_sent,
    ]
    .concat())
}
