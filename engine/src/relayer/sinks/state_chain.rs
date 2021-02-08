use anyhow::Result;
use std::marker::PhantomData;

use parity_scale_codec::Encode;
use substrate_subxt::{sp_core::U256, Call, Client, ClientBuilder, DefaultNodeRuntime};

use crate::relayer::{contracts::stake_manager::StakingEvent, EventSink};

#[derive(Clone)]
pub struct StateChainCaller<C: Call<DefaultNodeRuntime>> {
    phantom: PhantomData<C>,
    client: Client<DefaultNodeRuntime>,
}

impl<C: Call<DefaultNodeRuntime>> StateChainCaller<C> {
    pub async fn new(url: &str) -> Result<Self> {
        Ok(Self {
            phantom: PhantomData,
            client: ClientBuilder::<DefaultNodeRuntime>::new()
                .set_url(url)
                .build()
                .await?,
        })
    }
}

#[async_trait]
impl<E, C> EventSink<E> for StateChainCaller<C>
where
    E: 'static + Send,
    C: Call<DefaultNodeRuntime> + From<E> + std::fmt::Debug + Send + Sync,
{
    async fn process_event(&self, event: E) {
        let call = C::from(event);

        log::debug!("Encoded event call as: {:?}", call);

        let unsigned_extrinsic = self.client.create_unsigned(call).unwrap();

        let receipt = self
            .client
            .submit_extrinsic(unsigned_extrinsic)
            .await
            .unwrap();

        log::debug!("Extrinsic submitted, hash: {:?}", receipt);
    }
}

/// These need to be compatible with the types used by the runtime pallet.
type ValidatorId = U256;
type StakingAmount = u128;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode)]
pub enum StakingCall {
    Staked {
        node_id: ValidatorId,
        amount: StakingAmount,
    },
}

impl From<StakingEvent> for StakingCall {
    fn from(event: StakingEvent) -> Self {
        match event {
            StakingEvent::Staked { node_id, amount } => Self::Staked {
                node_id: unsafe { std::mem::transmute(node_id) },
                amount: amount.as_u128(),
            },
        }
    }
}

impl Call<DefaultNodeRuntime> for StakingCall {
    const MODULE: &'static str = "Staking";
    const FUNCTION: &'static str = "handle_staking_event";
}
