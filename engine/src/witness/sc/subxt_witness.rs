use anyhow::Result;
use frame_system::Event;
use futures::stream::Scan;
use pallet_cf_transactions::Event;
use sp_core::{sr25519, Pair};
use sp_keyring::AccountKeyring;
use state_chain_runtime::{AccountId, System};
use substrate_subxt::{
    balances::{TransferCallExt, TransferEvent},
    extrinsic::DefaultExtra,
    register_default_type_sizes,
    sp_core::Decode,
    sp_runtime::MultiSignature,
    system::ExtrinsicSuccessEvent,
    Client, ClientBuilder, EventSubscription, ExtrinsicSuccess, NodeTemplateRuntime, PairSigner,
    Runtime,
};

use crate::witness::sc::transactions::DataAddedEvent;

use super::{runtime::StateChainRuntime, transactions::DataAddedMoreEvent};

pub async fn start() {
    println!("Start the state chain witness with subxt");

    subscribe_to_events().await.unwrap();
}

async fn create_subscription<'a, E: substrate_subxt::Event<StateChainRuntime>>(
    client: Client<StateChainRuntime>,
) -> Result<EventSubscription<'a, StateChainRuntime>> {
    let sub = client.subscribe_finalized_events().await?;
    let decoder = client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    sub.filter_event::<E>();
    Ok(sub)
}

// An error thrown at one point for something - looks useful
// thread 'witness::sc::subxt_witness::tests::run_test' panicked at 'called `Result::unwrap()` on an `Err` value: The following types do not have a type size registered: ["Transactions::SwapQuoteAdded::states::SwapQuote", "Transactions::WitnessAdded::states::Witness", "Transactions::WithdrawRequestAdded::states::WithdrawRequest", "Transactions::OutputAdded::states::Output", "Transactions::PoolChangeAdded::states::PoolChange", "Transactions::OutputSentAdded::states::OutputSent", "Transactions::WithdrawAdded::states::Withdraw", "Transactions::DepositAdded::states::Deposit", "Transactions::DepositQuoteAdded::states::DepositQuote"] Use `ClientBuilder::register_type_size` to register missing type sizes.

pub async fn subscribe_to_events() -> Result<()> {
    // let signer: PairSigner::new(AccountKeyring::Alice.pair());

    let client = ClientBuilder::<StateChainRuntime>::new()
        .skip_type_sizes_check()
        .register_type_size::<AccountId>("AccountId")
        .build()
        .await?;

    // TODO: Put this back to finalized events
    let sub = client.subscribe_events().await?;
    let decoder = client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    let sub_more = client.subscribe_events().await?;
    let decoder_more = client.events_decoder();
    // let mut sub_more = EventSubscription::new(sub_more, decoder_more);

    // the loop will only decode these bois
    sub.filter_event::<DataAddedEvent<_>>();
    sub.filter_event::<DataAddedMoreEvent<_>>();
    // sub_more.filter_event::<DataAddedMoreEvent<_>>();
    loop {
        let raw = sub.next().await.unwrap().unwrap();
        println!("Raw event:\n{:#?}", raw);

        let event = DataAddedEvent::<StateChainRuntime>::decode(&mut &raw.data[..]).unwrap();
        println!(
            "The sender of this data is: {} and they sent: '{:?}'",
            event.who,
            String::from_utf8(event.clone().data),
        );

        println!("Here's the event to be added: {:#?}", event);

        // ===== next event subscription =====
        let raw = sub.next().await.unwrap().unwrap();
        println!("Raw event:\n{:#?}", raw);

        let event = DataAddedEvent::<StateChainRuntime>::decode(&mut &raw.data[..]).unwrap();
        println!(
            "The sender of this data is: {} and they sent: '{:?}'",
            event.who,
            String::from_utf8(event.clone().data),
        );

        println!("Here's the event to be added: {:#?}", event);
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
