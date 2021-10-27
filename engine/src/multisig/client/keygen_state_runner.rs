use std::sync::Arc;

use pallet_cf_vaults::CeremonyId;

use crate::{multisig::client::ThresholdParameters, p2p::AccountId};

use super::{
    common::{broadcast::BroadcastStage, CeremonyCommon, KeygenResult},
    keygen::{AwaitCommitments1, KeygenData, KeygenP2PSender},
    state_runner::StateRunner,
    utils::PartyIdxMapping,
    EventSender, KeygenResultInfo,
};

#[derive(Clone)]
pub struct KeygenStateRunner {
    inner: StateRunner<KeygenData, KeygenResult>,
    idx_mapping: Option<Arc<PartyIdxMapping>>,
    logger: slog::Logger,
}

impl KeygenStateRunner {
    pub fn new_unauthorised(logger: slog::Logger) -> Self {
        KeygenStateRunner {
            logger: logger.clone(),
            inner: StateRunner::new_unauthorised(logger),
            idx_mapping: None,
        }
    }

    pub fn on_keygen_request(
        &mut self,
        ceremony_id: CeremonyId,
        event_sender: EventSender,
        idx_mapping: Arc<PartyIdxMapping>,
        own_idx: usize,
        all_idxs: Vec<usize>,
    ) {
        self.idx_mapping = Some(idx_mapping.clone());

        let common = CeremonyCommon {
            ceremony_id,
            p2p_sender: KeygenP2PSender::new(
                idx_mapping.clone(),
                event_sender.clone(),
                ceremony_id,
            ),
            own_idx,
            all_idxs,
            logger: self.logger.clone(),
        };

        let processor = AwaitCommitments1::new(ceremony_id, common.clone());

        let stage = Box::new(BroadcastStage::new(processor, common));

        self.inner
            .on_ceremony_request(ceremony_id, stage, idx_mapping, event_sender);
    }

    pub fn try_expiring(&mut self) -> Option<Vec<AccountId>> {
        self.inner.try_expiring()
    }

    pub fn process_message(
        &mut self,
        sender_id: AccountId,
        data: KeygenData,
    ) -> Option<Result<KeygenResultInfo, Vec<AccountId>>> {
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
