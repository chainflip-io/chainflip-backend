use slog::o;
use substrate_subxt::{system::AccountStoreExt, Client, PairSigner, Signer};
use tokio::sync::mpsc::UnboundedReceiver;

use super::runtime::StateChainRuntime;
use crate::{
    eth::stake_manager::stake_manager::StakeManagerEvent,
    logging::COMPONENT_KEY,
};

use crate::state_chain::pallets::witness_api::*;

use anyhow::Result;

pub async fn start(
    signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair>,
    broadcast_stream : UnboundedReceiver<StakeManagerEvent>,
    subxt_client: Client<StateChainRuntime>,
    logger: &slog::Logger,
) {
    let mut sc_broadcaster = SCBroadcaster::new(signer, broadcast_stream, subxt_client, logger).await;

    sc_broadcaster
        .run()
        .await
        .expect("SC Broadcaster has died!");
}

pub struct SCBroadcaster {
    signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair>,
    broadcast_stream: UnboundedReceiver<StakeManagerEvent>,
    subxt_client: Client<StateChainRuntime>,
    logger: slog::Logger,
}

impl SCBroadcaster {
    pub async fn new(
        mut signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair>,
        broadcast_stream : UnboundedReceiver<StakeManagerEvent>,
        subxt_client: Client<StateChainRuntime>,
        logger: &slog::Logger,
    ) -> Self {
        let account_id = signer.account_id();
        let nonce = subxt_client
            .account(&account_id, None)
            .await
            .expect("Should be able to fetch account info")
            .nonce;
        let logger = logger.new(o!(COMPONENT_KEY => "SCBroadcaster"));
        slog::info!(logger, "Initial state chain nonce is: {}", nonce);
        signer.set_nonce(nonce);

        SCBroadcaster {
            signer,
            broadcast_stream,
            subxt_client,
            logger,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(event) = self.broadcast_stream.recv().await {
            self.submit_event(event).await?;
        }

        let err_msg = "State Chain Broadcaster has stopped running!";
        slog::error!(self.logger, "{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
    }

    /// Submit an event to the state chain, return the tx_hash
    async fn submit_event(&mut self, event: StakeManagerEvent) -> Result<()> {
        match event {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                return_addr,
                tx_hash,
            } => {
                slog::trace!(
                    self.logger,
                    "Sending witness_staked({:?}, {}, {:?}, {:?}) to state chain",
                    account_id,
                    amount,
                    return_addr,
                    tx_hash
                );
                self.subxt_client
                    .witness_staked(&self.signer, account_id, amount, tx_hash)
                    .await?;
                self.signer.increment_nonce();
            }
            StakeManagerEvent::ClaimExecuted {
                account_id,
                amount,
                tx_hash,
            } => {
                slog::trace!(
                    self.logger,
                    "Sending claim_executed({:?}, {}, {:?}) to the state chain",
                    account_id,
                    amount,
                    tx_hash
                );
                self.subxt_client
                    .witness_claimed(&self.signer, account_id, amount, tx_hash)
                    .await?;
                self.signer.increment_nonce();
            }
            StakeManagerEvent::MinStakeChanged { .. }
            | StakeManagerEvent::FlipSupplyUpdated { .. }
            | StakeManagerEvent::ClaimRegistered { .. } => {
                slog::warn!(
                    self.logger,
                    "{} is not to be submitted to the State Chain",
                    event
                );
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use super::*;

    use crate::{logging, settings};

    use sp_keyring::AccountKeyring;
    use sp_runtime::AccountId32;
    use substrate_subxt::ClientBuilder;

    const TX_HASH: [u8; 32] = [
        00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01, 02, 01, 02,
        01, 02, 01, 02, 01, 02, 01, 02, 01,
    ];

    async fn create_subxt_client(
        state_chain_settings: &settings::StateChain,
    ) -> Client<StateChainRuntime> {
        ClientBuilder::<StateChainRuntime>::new()
            .set_url(&state_chain_settings.ws_endpoint)
            .build()
            .await
            .expect(&format!(
                "Could not connect to state chain at: {}",
                &state_chain_settings.ws_endpoint
            ))
    }

    #[tokio::test]
    #[ignore = "depends on running mq and state chain"]
    async fn can_create_sc_broadcaster() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let subxt_client = create_subxt_client(&settings.state_chain).await;

        let logger = logging::test_utils::create_test_logger();

        let alice = AccountKeyring::Alice.pair();
        let pair_signer = PairSigner::new(alice);
        SCBroadcaster::new(pair_signer, tokio::sync::mpsc::unbounded_channel().1, subxt_client, &logger).await;
    }

    // TODO: Use the SC broadcaster struct instead
    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn submit_xt_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(&settings.state_chain).await;

        let alice = AccountKeyring::Alice.pair();
        let signer = PairSigner::new(alice);

        let tx_hash: [u8; 32] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01, 01, 01,
            01, 01, 01, 01, 01, 01, 01, 01, 01, 01,
        ];

        let result = subxt_client
            .witness_staked(
                &signer,
                AccountKeyring::Alice.to_account_id(),
                10000000u128,
                tx_hash,
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn sc_broadcaster_submit_event() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let subxt_client = create_subxt_client(&settings.state_chain).await;

        let alice = AccountKeyring::Alice.pair();
        let pair_signer = PairSigner::new(alice);
        let mut sc_broadcaster = SCBroadcaster::new(
            pair_signer,
            tokio::sync::mpsc::unbounded_channel().1, // TODO: Fix SCBroadcaster, so we don't need to initialise all this state to call submit_event (alastair holmes - 20.08.2021)
            subxt_client,
            &logging::test_utils::create_test_logger(),
        )
        .await;

        let staked_node_id =
            AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU").unwrap();
        let return_addr =
            web3::types::H160::from_str("0x73d669c173d88ccb01f6daab3a3304af7a1b22c1").unwrap();
        let staked_event = StakeManagerEvent::Staked {
            account_id: staked_node_id,
            amount: 100,
            return_addr: return_addr,
            tx_hash: TX_HASH,
        };

        let result = sc_broadcaster
            .submit_event(staked_event)
            .await;

        println!("Result: {:#?}", result);
    }
}
