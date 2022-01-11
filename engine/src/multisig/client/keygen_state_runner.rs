use std::{collections::BTreeSet, sync::Arc};

use pallet_cf_vaults::CeremonyId;
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::UnboundedSender;

use crate::{logging::KEYGEN_REQUEST_IGNORED, multisig::client::ThresholdParameters};

use super::{
    common::{broadcast::BroadcastStage, CeremonyCommon, KeygenResult},
    keygen::{AwaitCommitments1, HashContext, KeygenData, KeygenOptions},
    state_runner::StateRunner,
    utils::PartyIdxMapping,
    KeygenResultInfo, MultisigMessage, MultisigOutcomeSender,
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
        outgoing_p2p_message_sender: UnboundedSender<(AccountId, MultisigMessage)>,
        idx_mapping: Arc<PartyIdxMapping>,
        own_idx: usize,
        all_idxs: BTreeSet<usize>,
        keygen_options: KeygenOptions,
        context: HashContext,
        logger: &slog::Logger,
    ) {
        // We update the logger since it might contain additional context (in case
        // the ceremony was initially created before keygen request by a p2p message)
        self.logger = logger.clone();

        self.idx_mapping = Some(idx_mapping.clone());

        let common = CeremonyCommon {
            ceremony_id,
            outgoing_p2p_message_sender,
            validator_mapping: idx_mapping.clone(),
            own_idx,
            all_idxs,
            logger: self.logger.clone(),
        };

        let processor = AwaitCommitments1::new(common.clone(), keygen_options, context);

        let stage = Box::new(BroadcastStage::new(processor, common));

        if let Err(reason) =
            self.inner
                .on_ceremony_request(ceremony_id, stage, idx_mapping, outcome_sender)
        {
            slog::warn!(self.logger, #KEYGEN_REQUEST_IGNORED, "Keygen request ignored: {}", reason);
        }
    }

    /// Combine keygen result with the validator mapping for the current ceremony
    fn assemble_keygen_result_info(&self, result: KeygenResult) -> KeygenResultInfo {
        // NOTE: this line makes it impossible (currently) to create keys
        // for non-standard t/n ratios
        let params = ThresholdParameters::from_share_count(result.party_public_keys.len());

        let idx_mapping = self
            .idx_mapping
            .as_ref()
            .expect("idx mapping should be present")
            .clone();

        KeygenResultInfo {
            key: Arc::new(result),
            validator_map: idx_mapping,
            params,
        }
    }

    pub fn try_expiring(
        &mut self,
    ) -> Option<Result<KeygenResultInfo, (Vec<AccountId>, anyhow::Error)>> {
        self.inner
            .try_expiring()
            .map(|res| res.map(|keygen_result| self.assemble_keygen_result_info(keygen_result)))
    }

    pub fn process_message(
        &mut self,
        sender_id: AccountId,
        data: KeygenData,
    ) -> Option<Result<KeygenResultInfo, (Vec<AccountId>, anyhow::Error)>> {
        self.inner
            .process_message(sender_id, data)
            .map(|res| res.map(|keygen_result| self.assemble_keygen_result_info(keygen_result)))
    }

    /// returns true if the ceremony is authorized (has received a ceremony request)
    pub fn is_authorized(&self) -> bool {
        self.inner.is_authorized()
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
