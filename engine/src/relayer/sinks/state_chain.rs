use anyhow::Result;
use core::marker::PhantomData;

use parity_scale_codec::Encode;

use substrate_subxt::{
    module, sp_core::U256, system::System, Call, Client, ClientBuilder, Encoded,
    NodeTemplateRuntime,
};

use crate::relayer::{contracts::stake_manager::StakingEvent, EventSink};

pub type StakeManagerRuntimeCaller =
    StateChainCaller<stake_manager::HandleStakingEventCall<NodeTemplateRuntime>>;

#[derive(Clone)]
pub struct StateChainCaller<C: Call<NodeTemplateRuntime>> {
    phantom: PhantomData<C>,
    client: Client<NodeTemplateRuntime>,
}

impl<C: Call<NodeTemplateRuntime>> StateChainCaller<C> {
    pub async fn new(url: &str) -> Result<Self> {
        Ok(Self {
            phantom: PhantomData,
            client: ClientBuilder::<NodeTemplateRuntime>::new()
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
    C: Call<NodeTemplateRuntime> + From<E> + std::fmt::Debug + Send + Sync,
{
    async fn process_event(&self, event: E) {
        let call = C::from(event);

        log::debug!("Encoded event call as: {:?}", call);
        log::debug!("Encoded as hex this is: {}", hex::encode(call.encode()));

        let unsigned_extrinsic = self.client.create_unsigned(call).unwrap();

        match self.client.submit_extrinsic(unsigned_extrinsic).await {
            Ok(receipt) => log::info!("Extrinsic submitted, hash: {:?}", receipt),
            Err(e) => log::error!("Extrinsic rejected: {}", e),
        }

        // log::debug!("Extrinsic submitted, hash: {:?}", receipt);
    }
}

pub mod stake_manager {
    use core::marker::PhantomData;
    use parity_scale_codec::Encode;
    use substrate_subxt::{
        module,
        sp_core::U256,
        system::{System, SystemEventsDecoder},
        Call, Client, ClientBuilder, Encoded, NodeTemplateRuntime,
    };

    use crate::relayer::contracts::stake_manager::StakingEvent;

    /// These need to be compatible with the types used by the runtime pallet.
    type ValidatorId = U256;
    type StakingAmount = u128;

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Encode)]
    enum StakingCall {
        staked(ValidatorId, StakingAmount),
    }

    /// Converts an Ethereum `StakingEvent` to a `StakingCall`. Note this uses `unsafe` internally to convert
    /// from `ethabi::Uint` to `subxt::sp_core::U256`. This should be fine since these are based
    /// on the same implementation, just that the compiler can't tell.
    impl<T: StakeManager> From<StakingEvent> for HandleStakingEventCall<T> {
        fn from(event: StakingEvent) -> Self {
            let call = match event {
                StakingEvent::Staked { node_id, amount } => {
                    StakingCall::staked(unsafe { std::mem::transmute(node_id) }, amount.as_u128())
                }
            };
            HandleStakingEventCall {
                _runtime: PhantomData,
                call: Encoded(call.encode()),
            }
        }
    }

    /// The subset of the `pallet_cf_staking::Trait` that a client must implement.
    #[module]
    pub trait StakeManager: System {}

    impl StakeManager for NodeTemplateRuntime {}

    #[derive(Clone, Debug, PartialEq, Eq, Call, Encode)]
    pub struct HandleStakingEventCall<T: StakeManager> {
        /// Runtime marker.
        pub _runtime: PhantomData<T>,
        /// Encoded transaction.
        call: Encoded,
    }

    // impl Call<DefaultNodeRuntime> for HandleStakingEvent {
    //     const MODULE: &'static str = "StakeManager";
    //     const FUNCTION: &'static str = "handle_staking_event";
    // }
}
