use anyhow::Result;
use pallet_cf_transactions::Event;
use sp_core::{sr25519, Pair};
use sp_keyring::AccountKeyring;
use substrate_subxt::{
    balances::{TransferCallExt, TransferEvent},
    sp_core::Decode,
    Client, ClientBuilder, DefaultNodeRuntime, EventSubscription, PairSigner,
};

// Provides hooks to register missing runtime type sizes either statically via the Runtime trait impl:

// impl Runtime for MyCustomRuntime {
//     type Signature = MultiSignature;
//     type Extra = DefaultExtra<Self>;

//     fn register_type_sizes(event_type_registry: &mut EventTypeRegistry<Self>) {
//         event_type_registry.with_system();
//         event_type_registry.with_balances();
//         register_default_type_sizes(event_type_registry);
//         // add more custom type registrations here:
//         event_type_registry.register_type_size::<u32>("MyCustomType");
//     }
// }
// Or dynamically via the ClientBuilder:

// ClientBuilder::<NodeTemplateRuntime>::new()
//     .register_type_size::<u32>("MyCustomRuntimeType")
//     .build()
// In addition, the build() method on the ClientBuilder will now check for missing type sizes by default, and return an Err if there are any missing. This can be disabled by skip_type_sizes_check:

// ClientBuilder::<NodeTemplateRuntime>::new()
//     .skip_type_sizes_check()
//     .build()

pub async fn start() {
    println!("Start the state chain witness with subxt");

    subscribe_to_events().await.unwrap();
}

pub async fn subscribe_to_events() -> Result<()> {
    // let signer: PairSigner::new(AccountKeyring::Alice.pair());

    let client = ClientBuilder::new()
        // .set_url("http://127.0.0.1:9944")
        .skip_type_sizes_check()
        .build()
        .await?;

    let sub = client.subscribe_finalized_events().await?;
    let decoder = client.events_decoder();
    let mut sub = EventSubscription::<DefaultNodeRuntime>::new(sub, decoder);

    loop {
        let raw = sub.next().await.unwrap().unwrap();
        println!("Raw event: {:#?}", raw);
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn run_test() {
        start().await;
    }
}
