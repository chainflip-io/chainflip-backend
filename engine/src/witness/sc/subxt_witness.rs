use anyhow::Result;
use futures::stream::Scan;
use pallet_cf_transactions::Event;
use sp_core::{sr25519, Pair};
use sp_keyring::AccountKeyring;
use substrate_subxt::{
    balances::{TransferCallExt, TransferEvent},
    extrinsic::DefaultExtra,
    register_default_type_sizes,
    sp_core::Decode,
    sp_runtime::MultiSignature,
    system::ExtrinsicSuccessEvent,
    ClientBuilder, EventSubscription, ExtrinsicSuccess, NodeTemplateRuntime, PairSigner, Runtime,
};

use crate::witness::sc::transactions::DataAddedEvent;

use super::runtime::StateChainRuntime;

// impl Runtime for SCRuntime {
//     type Signature = MultiSignature;

//     type Extra = DefaultExtra<Self>;

//     fn register_type_sizes(event_type_registry: &mut substrate_subxt::EventTypeRegistry<Self>) {
//         event_type_registry.with_system();
//         register_default_type_sizes(event_type_registery);

//         // custom types
//         // event_type_registry.register_type_size(name)
//     }
// }

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

// }

pub async fn start() {
    println!("Start the state chain witness with subxt");

    subscribe_to_events().await.unwrap();
}

// An error thrown at one point for something - looks useful
// thread 'witness::sc::subxt_witness::tests::run_test' panicked at 'called `Result::unwrap()` on an `Err` value: The following types do not have a type size registered: ["Transactions::SwapQuoteAdded::states::SwapQuote", "Transactions::WitnessAdded::states::Witness", "Transactions::WithdrawRequestAdded::states::WithdrawRequest", "Transactions::OutputAdded::states::Output", "Transactions::PoolChangeAdded::states::PoolChange", "Transactions::OutputSentAdded::states::OutputSent", "Transactions::WithdrawAdded::states::Withdraw", "Transactions::DepositAdded::states::Deposit", "Transactions::DepositQuoteAdded::states::DepositQuote"] Use `ClientBuilder::register_type_size` to register missing type sizes.

pub async fn subscribe_to_events() -> Result<()> {
    // let signer: PairSigner::new(AccountKeyring::Alice.pair());

    let client = ClientBuilder::<StateChainRuntime>::new()
        // .set_url("http://127.0.0.1:9944")
        .skip_type_sizes_check()
        // .register_type_size::<u32>("AccountId32")
        // .register_type_size(name)
        .build()
        .await?;

    let sub = client.subscribe_finalized_events().await?;
    let decoder = client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);

    sub.filter_event::<DataAddedEvent<_>>();

    // try get just the frame system event
    loop {
        // TODO: DECODE THE EVENTS HERE YEET
        let raw = sub.next().await.unwrap().unwrap();
        println!("Raw event:\n{:#?}", raw);

        // this is how we decode, but how to do it with a custom type??
        // let event = DataAddedEvent::<StateChainRuntime>::decode(&mut &raw.data[..]).unwrap();

        // let event = DataAddedEvent::<StateChainRuntime>::decode(&mut &raw.data[..]).unwrap();

        // println!("Here's the decoded event");

        // println!("Here's the event to be added: {:#?}", event);
        // let event = TransferEvent::<NodeTemplateRuntime>::decode(&mut &raw.data[..]);
        // println!("Event metadata from frame system: {:#?}", event);
        // let event = TransferEvent::<DefaultNodeRuntime>::decode(&mut &raw.data[..]);
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

// RawEvent {
//     module: "Transactions",
//     variant: "DataAdded",
//     data: "8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480c617364",
// }

// RawEvent {
//     module: "System",
//     variant: "ExtrinsicSuccess",
//     data: "482d7c09000000000200",
// }
// Here's the event to be added: ExtrinsicSuccessEvent {
//     _runtime: PhantomData,
//     info: DispatchInfo {
//         weight: 159133000,
//         class: DispatchClass::Mandatory,
//         pays_fee: Pays::Yes,
//     },
// }
