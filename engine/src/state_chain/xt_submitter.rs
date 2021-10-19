use jsonrpc_core::{Error, ErrorCode};
use jsonrpc_core_client::RpcError;
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

use super::client::{IStateChainClient, StateChainClient};

type RetryCount = u8;

// The maximum number of times we retry submitting an extrinsic
const MAX_XT_RETRIES: RetryCount = 3;

/// Wraps the nonce, to provide a safe, atomic interface to the nonce
/// The nonce *can* be updated in two places:
/// 1. The XtSubmitter
/// 2. The SCObserver on every block (if the nonce from the previous block is greater than that of the last block)
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

/// Controls submission of extrinsics, so that we maintain a good nonce
pub struct XtSubmitter {
    to_retry: VecDeque<(RetryCount, Call)>,
    state_chain_client: Arc<StateChainClient>,
    xt_receiver: UnboundedReceiver<Call>,
    nonce: Arc<AtomicNonce>,
    logger: slog::Logger,
}

impl XtSubmitter {
    pub fn new(
        state_chain_client: Arc<StateChainClient>,
        xt_receiver: UnboundedReceiver<Call>,
        nonce: Arc<AtomicNonce>,
        logger: &slog::Logger,
    ) -> Self {
        Self {
            to_retry: VecDeque::new(),
            state_chain_client,
            xt_receiver,
            nonce,
            logger: logger.new(o!(COMPONENT_KEY => "XtSubmitter")),
        }
    }

    /// Starts the extrinsic submitter, which accepts extriniscs through a channel
    /// tracks the nonce, and submits using the correct nonce
    pub async fn start(&mut self) {
        while let Some(call) = self.xt_receiver.recv().await {
            // drain the failed transactions first
            while let Some((failed_count, failed_call)) = self.to_retry.pop_front() {
                if failed_count > MAX_XT_RETRIES {
                    continue;
                }
                self.submit_helper(failed_call, failed_count).await;
            }
            self.submit_helper(call, 0).await;
        }
    }

    // We increment the nonce in 2 scenarios:
    // 1. We successfully submit the transaction
    // 2. We receive an error to say that our nonce was incorrect
    async fn submit_helper(&mut self, call: Call, fail_count: u8) {
        match self
            .state_chain_client
            .submit_extrinsic_with_nonce(self.nonce.as_u32(), call.clone())
            .await
        {
            Ok(tx_hash) => {
                slog::trace!(
                    self.logger,
                    "Successfully submitted extrinsic with tx_hash: {}",
                    tx_hash
                );
                self.nonce.increment_nonce();
            }
            Err(err) => match err {
                RpcError::JsonRpcError(e) => match e {
                    Error {
                        code: ErrorCode::ServerError(1014),
                        ..
                    } => {
                        slog::warn!(self.logger, "Extrinsic submission failed with nonce error");
                        self.nonce.increment_nonce();
                        self.to_retry.push_back((fail_count + 1, call))
                    }
                    err => {
                        slog::error!(self.logger, "Failed to submit extrinsic: {}", err);
                    }
                },
                err => {
                    slog::error!(self.logger, "Failed to submit extrinsic: {}", err);
                }
            },
        };
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
