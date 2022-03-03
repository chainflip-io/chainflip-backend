use std::{collections::BTreeSet, iter::FromIterator, sync::Arc};

use slog::o;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot,
};

use crate::{
    logging::{CEREMONY_ID_KEY, COMPONENT_KEY},
    state_chain::client::{StateChainClient, StateChainRpcApi},
};

use super::{KeygenOutcome, MultisigInstruction, MultisigOutcome, SigningOutcome};

async fn process_multisig_outcome<RpcClient>(
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    multisig_outcome: MultisigOutcome,
    logger: &slog::Logger,
) where
    RpcClient: StateChainRpcApi,
{
    match multisig_outcome {
        MultisigOutcome::Signing(SigningOutcome { id, result }) => match result {
            Ok(sig) => {
                let _result = state_chain_client
                    .submit_unsigned_extrinsic(
                        pallet_cf_threshold_signature::Call::signature_success(id, sig.into()),
                        logger,
                    )
                    .await;
            }
            Err((err, bad_account_ids)) => {
                slog::error!(
                    logger,
                    "Signing ceremony failed with error: {:?}",
                    err;
                    CEREMONY_ID_KEY => id,
                );

                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_threshold_signature::Call::report_signature_failed_unbounded(
                            id,
                            bad_account_ids.into_iter().collect(),
                        ),
                        logger,
                    )
                    .await;
            }
        },
        MultisigOutcome::Keygen(KeygenOutcome { id, result }) => match result {
            Ok(pubkey) => {
                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_vaults::Call::report_keygen_outcome(
                            id,
                            pallet_cf_vaults::KeygenOutcome::Success(
                                cf_chains::eth::AggKey::from_pubkey_compressed(pubkey.serialize()),
                            ),
                        ),
                        logger,
                    )
                    .await;
            }
            Err((err, bad_account_ids)) => {
                slog::error!(
                    logger,
                    "Keygen ceremony failed with error: {:?}",
                    err;
                    CEREMONY_ID_KEY => id,
                );
                let _result = state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_vaults::Call::report_keygen_outcome(
                            id,
                            pallet_cf_vaults::KeygenOutcome::Failure(BTreeSet::from_iter(
                                bad_account_ids,
                            )),
                        ),
                        logger,
                    )
                    .await;
            }
        },
    };
}

pub async fn start<RpcClient>(
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    mut multisig_instruction_receiver: UnboundedReceiver<MultisigInstruction>,
    mut multisig_outcome_receiver: UnboundedReceiver<MultisigOutcome>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    mut shutdown: oneshot::Receiver<()>,
    logger: &slog::Logger,
) where
    RpcClient: StateChainRpcApi,
{
    let logger = logger.new(o!(COMPONENT_KEY => "KeygenVerification"));

    loop {
        tokio::select! {

          Some(instruction) = multisig_instruction_receiver.recv() => {

            multisig_instruction_sender.send(instruction).expect("receiver should exist");

          }
          option_multisig_outcome = multisig_outcome_receiver.recv() => {

              match option_multisig_outcome {
                  Some(outcome) => {
                    process_multisig_outcome(state_chain_client.clone(), outcome, &logger).await;
                  },
                  None => {
                      slog::error!(logger, "Exiting as multisig_outcome channel ended");
                      break;
                  }
              }
          }
          Ok(()) = &mut shutdown => {
              slog::info!(logger, "Received shutdown signal, exiting...");
              break;
          }

        }
    }
}
