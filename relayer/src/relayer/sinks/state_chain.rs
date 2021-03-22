use anyhow::Result;
use async_trait::async_trait;
use stake_manager::StakedCall;

use substrate_subxt::{extrinsic, Client, ClientBuilder, NodeTemplateRuntime};

use crate::relayer::{contracts::stake_manager::StakingEvent, EventSink};

/// Wraps a `subxt` substrate client.
#[derive(Clone)]
pub struct StateChainCaller {
    client: Client<NodeTemplateRuntime>,
}

impl StateChainCaller {
    /// Initialises the substrate client. Times out after 10 seconds if no connection can be made.
    /// The `url` argument should be a websocket url, for example "ws://localhost:9944".
    pub async fn new(url: &str) -> Result<Self> {
        Ok(Self {
            client: ClientBuilder::<NodeTemplateRuntime>::new()
                .set_url(url)
                // See https://github.com/paritytech/substrate-subxt/pull/227
                // This allows us to avoid defining every single call in the runtime. 
                .skip_type_sizes_check()
                .build()
                .await?,
        })
    }
}

/// An EventSink implementation that accepts a StakingEvent and calls the corresponding extrinsic on the
/// state chain.
#[async_trait]
impl EventSink<StakingEvent> for StateChainCaller {
    async fn process_event(&self, event: StakingEvent) -> Result<()> {
        let call_encoded = self.client.encode(match event {
            StakingEvent::Staked(node_id, amount) => StakedCall::from_eth_params(node_id, amount),
        })?;

        log::debug!("Encoded event call as: {}", hex::encode(&call_encoded.0));

        let unsigned_extrinsic = extrinsic::create_unsigned::<NodeTemplateRuntime>(call_encoded);

        match self.client.submit_extrinsic(unsigned_extrinsic).await {
            Ok(receipt) => log::info!("Extrinsic submitted, hash: {:?}", receipt),
            Err(e) => log::error!("Extrinsic rejected: {}", e),
        };

        Ok(())
    }
}

pub mod stake_manager {
    use core::marker::PhantomData;
    use parity_scale_codec::Encode;
    use substrate_subxt::{
        module,
        sp_core::U256,
        system::System,
        Call, NodeTemplateRuntime,
    };
    use web3::ethabi;

    /// These need to be compatible with the types used by the runtime pallet.
    type ValidatorId = U256;
    type StakingAmount = u128;

    /// The subset of the `pallet_cf_staking::Config` that a client must implement.
    #[module]
    pub trait StakeManager: System {}

    impl StakeManager for NodeTemplateRuntime {}

    #[derive(Clone, Debug, PartialEq, Eq, Call, Encode)]
    pub struct StakedCall<T: StakeManager> {
        /// Runtime marker.
        pub _runtime: PhantomData<T>,
        /// Call arguments.
        pub account_id: ValidatorId,
        pub amount: StakingAmount,
    }

    impl<T: StakeManager> StakedCall<T> {
        /// Used to convert ethereum event params to params for the runtime call. Note this uses
        /// `unsafe` to convert from `ethabi::Uint` to `subxt::sp_core::U256`. This should be fine
        /// since these are based on the same implementation, just that the compiler doesn't know this.
        pub fn from_eth_params(node_id: ethabi::Uint, amount: ethabi::Uint) -> Self {
            Self {
                _runtime: PhantomData,
                account_id: unsafe { std::mem::transmute(node_id) },
                amount: amount.as_u128(),
            }
        }
    }
}
