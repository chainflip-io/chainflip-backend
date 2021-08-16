use chainflip_engine::{
    eth::{
        self,
        key_manager::key_manager::{ChainflipKey, KeyManagerEvent},
    },
    logging::utils,
    mq::{nats_client::NatsMQClient, IMQClient, Subject},
    settings::Settings,
};

use futures::stream::StreamExt;

#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::create_cli_logger();

    let settings = Settings::from_file("config/Testing.toml").unwrap();

    let mq_c = NatsMQClient::new(&settings.message_queue).await.unwrap();

    // subscribe before the witness pushes events to the queue
    let km_event_stream = mq_c
        .subscribe::<KeyManagerEvent>(Subject::KeyManager)
        .await
        .unwrap();

    // The Key Manager Witness will run forever unless we stop it after a short time
    // in which it should have already done it's job.
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        eth::key_manager::start_key_manager_witness(&settings, mq_c, &root_logger),
    )
    .await;
    slog::info!(&root_logger, "Subscribed");

    // Grab the events from the stream and put them into a vec
    let km_events = km_event_stream
        .take_until(tokio::time::sleep(std::time::Duration::from_millis(1)))
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("Error in event stream");

    assert!(
        !km_events.is_empty(),
        "Event stream was empty. Have you ran the setup script to deploy/run the contracts?"
    );

    // The following event details correspond to the events in chainflip-eth-contracts/scripts/deploy_and.py
    km_events
        .iter()
        .find(|event| match event {
            KeyManagerEvent::KeyChange {
                signed,
                old_key,
                new_key,
                ..
            } => {
                // See if the key change event matches 1 of the 3 events in the 'deploy_and.py' script
                // All the key strings in this test are decimal versions of the hex strings in the consts.py script
                // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py
                // TODO: Use hex strings instead of dec strings. So we can use the exact const hex strings from consts.py.
                
                if new_key == &ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap(){

                    assert_eq!(signed,&true);
                    assert_eq!(old_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                    return true

                } else if new_key == &ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap(){

                    assert_eq!(signed,&false);
                    assert_eq!(old_key,&ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                    return true

                 } else if new_key == &ChainflipKey::from_dec_str("35388971693871284788334991319340319470612669764652701045908837459480931993848",false).unwrap(){

                    assert_eq!(signed,&false);
                    assert_eq!(old_key,&ChainflipKey::from_dec_str("29963508097954364125322164523090632495724997135004046323041274775773196467672",true).unwrap());
                    return true

                } else {
                    panic!("KeyChange event with unexpected key: {:?}", new_key);
                }
            }
        }
        ).unwrap();
}
