use slog::o;
use state_chain_runtime::Call;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::logging::COMPONENT_KEY;

use super::client::StateChainClient;

type RetryCount = u8;

// The maximum number of times we retry submitting an extrinsic
const MAX_XT_RETRIES: RetryCount = 3;

/// Wraps the nonce, to provide a safe, atomic interface to the nonce
#[derive(Debug)]
pub struct AtomicNonce(AtomicU32);

impl AtomicNonce {
    pub fn new(nonce: u32) -> Self {
        Self(AtomicU32::new(nonce))
    }

    pub fn as_u32(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn increment_nonce(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_best_nonce(&self, nonce_candidate: u32) {
        self.0.fetch_max(nonce_candidate, Ordering::Relaxed);
    }
}

// 1. Remove the assumption that all failures are due to nonce failures

// We are currently assuming that all failures are nonce failures
// is this a safe assumption?

/// Starts the extrinsic submitter, which accepts extriniscs through a channel
/// tracks the nonce, and submits using the correct nonce
pub async fn start(
    state_chain_client: Arc<StateChainClient>,
    nonce: Arc<AtomicNonce>,
    mut xt_receiver: UnboundedReceiver<Call>,
    logger: &slog::Logger,
) {
    let mut xts_to_retry: VecDeque<(RetryCount, Call)> = VecDeque::new();
    let logger = logger.new(o!(COMPONENT_KEY => "XtSubmitter"));

    // TODO: Think about how we might be able to remove some duplication here
    // this is the loop that sends only the new xts
    while let Some(call) = xt_receiver.recv().await {
        match state_chain_client
            .submit_extrinsic_with_nonce(nonce.as_u32(), call.clone())
            .await
        {
            Ok(tx_hash) => {
                slog::trace!(
                    logger,
                    "Successfully submitted extrinsic with tx_hash: {}",
                    tx_hash
                );
            }
            Err(err) => {
                slog::error!(logger, "Failed to submit extrinsic: {}", err);
                xts_to_retry.push_back((0, call))
            }
        }

        // retry failures right away
        while let Some((fail_count, failed_call)) = xts_to_retry.pop_front() {
            if fail_count >= MAX_XT_RETRIES {
                continue;
            }
            match state_chain_client
                .submit_extrinsic_with_nonce(nonce.as_u32(), failed_call.clone())
                .await
            {
                Ok(tx_hash) => {
                    slog::trace!(
                        logger,
                        "Successfully submitted extrinsic with tx_hash: {}",
                        tx_hash
                    );
                }
                Err(err) => {
                    slog::error!(logger, "Failed to submit extrinsic: {}", err);
                    xts_to_retry.push_back((fail_count + 1, failed_call))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_xt_submitter() {
        todo!()
    }
}
