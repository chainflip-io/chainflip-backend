use crate::{
    common::api::ResponseError, common::StakerId, quoter::StateProvider, side_chain::SideChainTx,
};
use chainflip_common::types::{chain::Output, UUIDv4};
use itertools::Itertools;
use reqwest::StatusCode;
use serde::Deserialize;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

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
    if params.quote_id.is_some() && params.staker_id.is_some() {
        return Err(ResponseError::new(
            StatusCode::BAD_REQUEST,
            "Only one of quoterId or stakerId is allowed",
        ));
    }

    if let Some(quote_id) = params.quote_id {
        let id = match UUIDv4::from_str(&quote_id) {
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

/// Get transactions related to the given quote id
fn get_quote_id_transactions<S>(
    id: UUIDv4,
    state: Arc<Mutex<S>>,
) -> Result<Vec<SideChainTx>, ResponseError>
where
    S: StateProvider,
{
    let state = state.lock().unwrap();

    let witnesses = state.get_witnesses();
    let outputs = state.get_outputs();
    let sent = state.get_output_sents();
    let deposits = state.get_deposits();

    drop(state);

    // I know this is terribly inefficient but it'll have to do for now until we can clean it up :(

    let filtered_witnesses: Vec<SideChainTx> = witnesses
        .into_iter()
        .filter(|tx| tx.quote == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_deposit: Vec<SideChainTx> = deposits
        .into_iter()
        .filter(|tx| tx.quote == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_outputs: Vec<Output> = outputs
        .into_iter()
        .filter(|tx| tx.parent_id() == id)
        .collect();
    let ids: Vec<UUIDv4> = filtered_outputs.iter().map(|tx| tx.id).collect();
    let filtered_outputs: Vec<SideChainTx> =
        filtered_outputs.into_iter().map(|tx| tx.into()).collect();

    let filtered_output_sent: Vec<SideChainTx> = sent
        .into_iter()
        .filter(|tx| ids.iter().find(|id| tx.outputs.contains(id)).is_some())
        .map(|tx| tx.into())
        .collect();

    Ok([
        filtered_witnesses,
        filtered_deposit,
        filtered_outputs,
        filtered_output_sent,
    ]
    .concat())
}

/// Get transactions related to the given staker id
fn get_staker_id_transactions<S>(
    id: StakerId,
    state: Arc<Mutex<S>>,
) -> Result<Vec<SideChainTx>, ResponseError>
where
    S: StateProvider,
{
    let state = state.lock().unwrap();

    let quotes = state.get_deposit_quotes();
    let witnesses = state.get_witnesses();
    let outputs = state.get_outputs();
    let sent = state.get_output_sents();
    let deposits = state.get_deposits();
    let withdraw_requests = state.get_withdraw_requests();
    let withdraws = state.get_withdraws();

    drop(state);

    let quotes = quotes
        .into_iter()
        .filter(|tx| tx.staker_id == id)
        .collect_vec();
    let filtered_withdraw_requests = withdraw_requests
        .into_iter()
        .filter(|tx| tx.staker_id == id)
        .collect_vec();

    let filtered_withdraws: Vec<SideChainTx> = withdraws
        .into_iter()
        .filter(|tx| {
            filtered_withdraw_requests
                .iter()
                .find(|req| req.id == tx.withdraw_request)
                .is_some()
        })
        .map(|tx| tx.into())
        .collect();

    let filtered_witnesses: Vec<SideChainTx> = witnesses
        .into_iter()
        .filter(|tx| quotes.iter().find(|quote| tx.quote == quote.id).is_some())
        .map(|tx| tx.into())
        .collect();

    let filtered_outputs: Vec<Output> = outputs
        .into_iter()
        .filter(|tx| {
            let withdraw_output = filtered_withdraw_requests
                .iter()
                .find(|quote| quote.id == tx.parent_id())
                .is_some();

            let refund_output = quotes
                .iter()
                .find(|quote| quote.id == tx.parent_id())
                .is_some();

            withdraw_output || refund_output
        })
        .collect();
    let ids: Vec<UUIDv4> = filtered_outputs.iter().map(|tx| tx.id).collect();
    let filtered_outputs: Vec<SideChainTx> =
        filtered_outputs.into_iter().map(|tx| tx.into()).collect();

    let filtered_output_sent: Vec<SideChainTx> = sent
        .into_iter()
        .filter(|tx| ids.iter().find(|id| tx.outputs.contains(id)).is_some())
        .map(|tx| tx.into())
        .collect();

    let filtered_quotes: Vec<SideChainTx> = quotes.into_iter().map(|tx| tx.into()).collect();

    let filtered_deposits: Vec<SideChainTx> = deposits
        .into_iter()
        .filter(|tx| tx.staker_id == id)
        .map(|tx| tx.into())
        .collect();

    let filtered_withdraw_requests: Vec<SideChainTx> = filtered_withdraw_requests
        .into_iter()
        .map(|tx| tx.into())
        .collect();

    Ok([
        filtered_quotes,
        filtered_witnesses,
        filtered_deposits,
        filtered_withdraw_requests,
        filtered_withdraws,
        filtered_outputs,
        filtered_output_sent,
    ]
    .concat())
}

#[cfg(test)]
mod test {
    #[test]
    #[ignore = "todo"]
    fn test_returns_transactions_belonging_to_swap_quote() {
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn test_returns_transactions_belonging_to_deposit_quote() {
        todo!()
    }

    #[test]
    #[ignore = "todo"]
    fn test_returns_transactions_belonging_to_staker_id() {
        // Test quotes, witnesses, refund outputs, deposits, withdraw requests, withdraw outputs, output sent
        todo!()
    }
}
