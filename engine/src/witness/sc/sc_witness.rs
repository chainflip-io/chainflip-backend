use substrate_api_client::{events::EventsDecoder, sp_runtime::app_crypto::sr25519, Api};

use std::{convert::TryFrom, sync::mpsc::channel};
use substrate_api_client::utils::FromHexString;

use frame_system::EventRecord;
use sp_core::H256 as Hash;

use parity_scale_codec::Decode;
use state_chain_runtime::{AccountId, Event};

/// Start witnessing the state chain
pub fn start() {
    println!("Start the state chain witness");

    subscribe_to_events();
}

pub fn subscribe_to_events() {
    let url = "127.0.0.1:9944";

    let api = Api::<sr25519::Pair>::new(format!("ws://{}", url)).unwrap();

    let (events_in, events_out) = channel();
    api.subscribe_events(events_in).unwrap();

    let metadata = api.metadata;

    loop {
        let event_str = events_out.recv().unwrap();

        let _unhex = Vec::from_hex(event_str).unwrap();
        let mut _er_enc = _unhex.as_slice();

        let mut decoder = EventsDecoder::try_from(metadata.clone()).unwrap();

        decoder
            .register_type_size::<AccountId>("AccountId")
            .unwrap();
        // let decoder = decoder.register_type_size(&event_str[..]).unwrap();

        let _events = Vec::<EventRecord<Event, Hash>>::decode(&mut _er_enc);

        println!("Here are the events boi: {:#?}", _events);

        match _events {
            Ok(evts) => {
                for evr in &evts {
                    println!("Decoded: {:?} {:?}", evr.phase, evr.event);
                    match &evr.event {
                        Event::pallet_cf_transactions(be) => {
                            println!(">>>>>> Transactions event: {:?}", be);
                            // match &be {
                            //     pallet_cf_transactions::Event::DataAdded(
                            //         who,
                            //         data,
                            //     ) => {
                            //         println!("The person who added the data: {:?}", who);
                            //         println!("Data added: {:?}", data);
                            //         return;
                            //     }
                            //     _ => {
                            //         println!("Ignoring unsupported transactions event");
                            //     }
                            // }
                        }
                        _ => {
                            println!("Ignoring unsupported module event: {:?}", evr.event);
                        }
                    }
                }
            }
            Err(_) => {
                println!("Couldn't decode record list");
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn testing_stuff() {
        start();
    }
}
