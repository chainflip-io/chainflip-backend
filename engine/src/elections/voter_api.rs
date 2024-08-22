use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::composite::{self, Composite},
	vote_storage::{self, VoteStorage},
};

#[async_trait::async_trait]
pub trait VoterApi<E: ElectoralSystem> {
	async fn vote(
		&self,
		settings: <E as ElectoralSystem>::ElectoralSettings,
		properties: <E as ElectoralSystem>::ElectionProperties,
	) -> Result<<<E as ElectoralSystem>::Vote as VoteStorage>::Vote, anyhow::Error>;
}

pub struct CompositeVoter<ElectoralSystem, Voters> {
	voters: Voters,
	_phantom: core::marker::PhantomData<ElectoralSystem>,
}
impl<ElectoralSystem, Voters: Clone> Clone for CompositeVoter<ElectoralSystem, Voters> {
	fn clone(&self) -> Self {
		Self { voters: self.voters.clone(), _phantom: Default::default() }
	}
}

impl<ElectoralSystem, Voters> CompositeVoter<ElectoralSystem, Voters> {
	pub fn new(voters: Voters) -> Self {
		Self { voters, _phantom: Default::default() }
	}
}

macro_rules! generate_voter_api_tuple_impls {
    ($module:ident: ($(($electoral_system:ident, $voter:ident)),*$(,)?)) => {
        #[allow(non_snake_case)]
        #[async_trait::async_trait]
        impl<$($voter: VoterApi<$electoral_system> + Send + Sync),*, $($electoral_system : ElectoralSystem + Send + Sync + 'static),*, Hooks: Send + Sync + 'static + composite::$module::Hooks<$($electoral_system,)*>> VoterApi<Composite<($($electoral_system,)*), Hooks>> for CompositeVoter<Composite<($($electoral_system,)*), Hooks>, ($($voter,)*)> {
            async fn vote(
                &self,
                settings: <Composite<($($electoral_system,)*), Hooks> as ElectoralSystem>::ElectoralSettings,
                properties: <Composite<($($electoral_system,)*), Hooks> as ElectoralSystem>::ElectionProperties,
            ) -> Result<
                <<Composite<($($electoral_system,)*), Hooks> as ElectoralSystem>::Vote as VoteStorage>::Vote,
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
                            ).await.map(CompositeVote::$electoral_system)
                        },
                    )*
                }
            }
        }
    }
}

generate_voter_api_tuple_impls!(tuple_1_impls: ((A, A0)));
generate_voter_api_tuple_impls!(tuple_2_impls: ((A, A0), (B, B0)));
generate_voter_api_tuple_impls!(tuple_3_impls: ((A, A0), (B, B0), (C, C0)));
generate_voter_api_tuple_impls!(tuple_4_impls: ((A, A0), (B, B0), (C, C0), (D, D0)));
generate_voter_api_tuple_impls!(tuple_5_impls: ((A, A0), (B, B0), (C, C0), (D, D0), (EE, E0)));
