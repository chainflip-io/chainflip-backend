use substrate_subxt::{Client, PairSigner};
use tokio_stream::StreamExt;

use super::{helpers::create_subxt_client, runtime::StateChainRuntime};
use crate::{
    eth::stake_manager::stake_manager::StakeManagerEvent,
    mq::{pin_message_stream, IMQClient, IMQClientFactory, Subject},
    settings::Settings,
};

use crate::state_chain::staking::{WitnessClaimedCallExt, WitnessStakedCallExt};

use anyhow::Result;

pub async fn start<IMQ, IMQF>(
    settings: &Settings,
    signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair>,
    mq_factory: IMQF,
) where
    IMQ: IMQClient + Sync + Send,
    IMQF: IMQClientFactory<IMQ>,
{
    let mq_client = *mq_factory
        .create()
        .await
        .expect("Should create message queue client");

    let sc_broadcaster = SCBroadcaster::new(&settings, signer, mq_client).await;

    sc_broadcaster
        .run()
        .await
        .expect("SC Broadcaster has died!");
}

pub struct SCBroadcaster<MQ>
where
    MQ: IMQClient + Send + Sync,
{
    mq_client: MQ,
    sc_client: Client<StateChainRuntime>,
    signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair>,
}

impl<MQ> SCBroadcaster<MQ>
where
    MQ: IMQClient + Send + Sync,
{
    pub async fn new(
        settings: &Settings,
        signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair>,
        mq_client: MQ,
    ) -> Self {
        let sc_client = create_subxt_client(settings.state_chain.clone())
            .await
            .unwrap();

        SCBroadcaster {
            mq_client,
            sc_client,
            signer,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let stream = self
            .mq_client
            .subscribe::<StakeManagerEvent>(Subject::StakeManager)
            .await?;

        let mut stream = pin_message_stream(stream);

        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => self.submit_event(event).await?,
                Err(e) => {
                    log::error!("Could not read event from StakeManager event stream: {}", e);
                    return Err(e);
                }
            }
        }

        let err_msg = "State Chain Broadcaster has stopped running!";
        log::error!("{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
    }

    /// Submit an event to the state chain, return the tx_hash
    async fn submit_event(&self, event: StakeManagerEvent) -> Result<()> {
        match event {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                tx_hash,
            } => {
                log::trace!(
                    "Sending witness_staked({:?}, {}, {:?}) to state chain",
                    account_id,
                    amount,
                    tx_hash
                );
                self.sc_client
                    .witness_staked(&self.signer, account_id, amount, tx_hash)
                    .await?;
            }
            StakeManagerEvent::ClaimExecuted {
                account_id,
                amount,
                tx_hash,
            } => {
                log::trace!(
                    "Sending claim_executed({:?}, {}, {:?}) to the state chain",
                    account_id,
                    amount,
                    tx_hash
                );
                self.sc_client
                    .witness_claimed(&self.signer, account_id, amount, tx_hash)
                    .await?;
            }
            StakeManagerEvent::MinStakeChanged { .. }
            | StakeManagerEvent::EmissionChanged { .. }
            | StakeManagerEvent::ClaimRegistered { .. } => {
                log::warn!("{} is not to be submitted to the State Chain", event);
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use super::*;

    use crate::{
        mq::{nats_client::NatsMQClientFactory, IMQClientFactory},
        settings::{self},
    };

    use sp_keyring::AccountKeyring;
    use sp_runtime::AccountId32;

    const TX_HASH: [u8; 32] = [
        00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01, 02, 01, 02,
        01, 02, 01, 02, 01, 02, 01, 02, 01,
    ];

    #[tokio::test]
    #[ignore = "depends on running mq and state chain"]
    async fn can_create_sc_broadcaster() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

        let mq_client = mq_factory
            .create()
            .await
            .expect("Could not create MQ client");

        let alice = AccountKeyring::Alice.pair();
        let pair_signer = PairSigner::new(alice);
        SCBroadcaster::new(&settings, pair_signer, *mq_client).await;
    }

    // TODO: Use the SC broadcaster struct instead
    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn submit_xt_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();

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

        println!("Result is: {:#?}", result);

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn sc_broadcaster_submit_event() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

        let mq_client = mq_factory
            .create()
            .await
            .expect("Could not create MQ client");

        let alice = AccountKeyring::Alice.pair();
        let pair_signer = PairSigner::new(alice);
        let sc_broadcaster = SCBroadcaster::new(&settings, pair_signer, *mq_client).await;

        let staked_node_id =
            AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU").unwrap();
        let staked_event = StakeManagerEvent::Staked {
            account_id: staked_node_id,
            amount: 100,
            tx_hash: TX_HASH,
        };

        let result = sc_broadcaster.submit_event(staked_event).await;

        println!("Result: {:#?}", result);
    }
}
