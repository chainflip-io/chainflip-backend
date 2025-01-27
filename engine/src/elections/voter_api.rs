use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	Parameter,
};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_system_runner::{ElectoralSystemRunner, RunnerStorageAccessTrait},
	electoral_systems::composite::{self, CompositeRunner},
	vote_storage::{self, VoteStorage},
	ElectoralSystemTypes, VoteOf,
};

#[async_trait::async_trait]
pub trait VoterApi<E: ElectoralSystem> {
	async fn vote(
		&self,
		settings: <E as ElectoralSystemTypes>::ElectoralSettings,
		properties: <E as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<E>>, anyhow::Error>;
}

pub struct CompositeVoter<ElectoralSystemRunner, Voters> {
	voters: Voters,
	_phantom: core::marker::PhantomData<ElectoralSystemRunner>,
}
impl<ElectoralSystemRunner, Voters: Clone> Clone for CompositeVoter<ElectoralSystemRunner, Voters> {
	fn clone(&self) -> Self {
		Self { voters: self.voters.clone(), _phantom: Default::default() }
	}
}

impl<ElectoralSystemRunner, Voters> CompositeVoter<ElectoralSystemRunner, Voters> {
	pub fn new(voters: Voters) -> Self {
		Self { voters, _phantom: Default::default() }
	}
}

#[async_trait::async_trait]
pub trait CompositeVoterApi<E: ElectoralSystemRunner> {
	async fn vote(
		&self,
		settings: <E as ElectoralSystemTypes>::ElectoralSettings,
		properties: <E as ElectoralSystemTypes>::ElectionProperties,
	) -> Result<Option<VoteOf<E>>, anyhow::Error>;
}

// TODO Combine this into the composite macro PRO-1736
macro_rules! generate_voter_api_tuple_impls {
    ($module:ident: ($(($electoral_system:ident, $voter:ident)),*$(,)?)) => {
        #[allow(non_snake_case)]
        #[async_trait::async_trait]
        impl<$($voter: VoterApi<$electoral_system> + Send + Sync),*, $($electoral_system : ElectoralSystem<ValidatorId = ValidatorId> + Send + Sync + 'static),*, ValidatorId: MaybeSerializeDeserialize + Member + Parameter, StorageAccess: RunnerStorageAccessTrait<ElectoralSystemRunner = CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, Hooks>> + Send + Sync + 'static, Hooks: Send + Sync + 'static + composite::$module::Hooks<$($electoral_system,)*>> CompositeVoterApi<CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, Hooks>> for CompositeVoter<CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, Hooks>, ($($voter,)*)> {
            async fn vote(
                &self,
                settings: <CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, Hooks> as ElectoralSystemTypes>::ElectoralSettings,
                properties: <CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, Hooks> as ElectoralSystemTypes>::ElectionProperties,
            ) -> Result<
            Option<
                <<CompositeRunner<($($electoral_system,)*), ValidatorId, StorageAccess, Hooks> as ElectoralSystemTypes>::VoteStorage as VoteStorage>::Vote
                >,
                anyhow::Error,
            > {
                use vote_storage::composite::$module::CompositeVote;
                use composite::$module::CompositeElectionProperties;

                let ($($voter,)*) = &self.voters;
                let ($($electoral_system,)*) = settings;
                match properties {
                    $(
                        CompositeElectionProperties::$electoral_system(properties) => {
                            $voter.vote(
                                $electoral_system,
                                properties,
                            ).await.map(|x| x.map(CompositeVote::$electoral_system))
                        },
                    )*
                }
            }
        }
    }
}

generate_voter_api_tuple_impls!(tuple_7_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0), (FF, F0), (GG, G0)));
