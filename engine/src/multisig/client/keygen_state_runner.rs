use std::{collections::BTreeSet, sync::Arc};

use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    multisig::client::ThresholdParameters,
    p2p::{AccountId, P2PMessage},
};

use super::{
    common::{broadcast::BroadcastStage, CeremonyCommon, KeygenResult},
    keygen::{AwaitCommitments1, HashContext, KeygenData, KeygenOptions},
    state_runner::StateRunner,
    utils::PartyIdxMapping,
    KeygenResultInfo, MultisigOutcomeSender,
};

#[derive(Clone)]
pub struct KeygenStateRunner {
    inner: StateRunner<KeygenData, KeygenResult>,
    idx_mapping: Option<Arc<PartyIdxMapping>>,
    logger: slog::Logger,
}

impl KeygenStateRunner {
    pub fn new_unauthorised(logger: &slog::Logger) -> Self {
        KeygenStateRunner {
            logger: logger.clone(),
            inner: StateRunner::new_unauthorised(logger),
            idx_mapping: None,
        }
    }

    pub fn on_keygen_request(
        &mut self,
        ceremony_id: CeremonyId,
        outcome_sender: MultisigOutcomeSender,
        outgoing_p2p_message_sender: UnboundedSender<P2PMessage>,
        idx_mapping: Arc<PartyIdxMapping>,
        own_idx: usize,
        all_idxs: BTreeSet<usize>,
        keygen_options: KeygenOptions,
        context: HashContext,
    ) {
        self.idx_mapping = Some(idx_mapping.clone());

        let common = CeremonyCommon {
            ceremony_id,
            outgoing_p2p_message_sender: outgoing_p2p_message_sender.clone(),
            validator_mapping: idx_mapping.clone(),
            own_idx,
            all_idxs,
            logger: self.logger.clone(),
        };

        let processor = AwaitCommitments1::new(common.clone(), keygen_options, context);

        let stage = Box::new(BroadcastStage::new(processor, common));

        self.inner
            .on_ceremony_request(ceremony_id, stage, idx_mapping, outcome_sender);
    }

    pub fn try_expiring(&mut self) -> Option<Vec<AccountId>> {
        self.inner.try_expiring()
    }

    pub fn process_message(
        &mut self,
        sender_id: AccountId,
        data: KeygenData,
    ) -> Option<Result<KeygenResultInfo, (Vec<AccountId>, anyhow::Error)>> {
        self.inner.process_message(sender_id, data).map(|res| {
            res.map(|keygen_result| {
                let params =
                    ThresholdParameters::from_share_count(keygen_result.party_public_keys.len());

                let idx_mapping = self
                    .idx_mapping
                    .as_ref()
                    .expect("idx mapping should be present")
                    .clone();

                KeygenResultInfo {
                    key: Arc::new(keygen_result),
                    validator_map: idx_mapping,
                    params,
                }
            })
        })
    }
}

#[cfg(test)]
impl KeygenStateRunner {
    pub fn get_stage(&self) -> Option<String> {
        self.inner.get_stage()
    }

    #[cfg(test)]
    pub fn set_expiry_time(&mut self, expiry_time: std::time::Instant) {
        self.inner.set_expiry_time(expiry_time)
    }
}
