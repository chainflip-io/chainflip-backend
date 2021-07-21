use std::marker::PhantomData;

use frame_support::Parameter;
use substrate_subxt::{module, system::System, Store};

use codec::Encode;

use std::fmt::Debug;

#[module]
pub trait Session: System {
    type ValidatorId: Parameter + Debug + Ord + Default + Send + Sync + 'static;
}

/// The current set of validators.
#[derive(Encode, Store, Debug)]
pub struct ValidatorsStore<T: Session> {
    #[store(returns = Vec<<T as Session>::ValidatorId>)]
    /// Marker for the runtime
    pub _runtime: PhantomData<T>,
}

#[cfg(test)]
mod tests {
    use crate::{settings, state_chain::helpers::create_subxt_client};

    use super::*;

    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn test_get_session_validators() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();

        let validators = subxt_client.validators(None).await;
        println!("the validators are here: {:?}", validators);
    }
}
