use chainflip_engine::{
    eth::{
        key_manager::{ChainflipKey, KeyManager, KeyManagerEvent},
        new_synced_web3_client,
    },
    logging::utils,
    settings::Settings,
};

use futures::stream::StreamExt;

mod common;

#[tokio::test]
pub async fn test_all_key_manager_events() {
    let root_logger = utils::create_cli_logger();

    let settings = Settings::from_file("config/Testing.toml").unwrap();

    let web3 = new_synced_web3_client(&settings, &root_logger)
        .await
        .unwrap();

    let key_manager = KeyManager::new(&settings).unwrap();

    // The stream is infinite unless we stop it after a short time
    // in which it should have already done it's job.
    let km_events = key_manager
        .event_stream(&web3, settings.eth.from_block, &root_logger)
        .await
        .unwrap()
        .take_until(tokio::time::sleep(std::time::Duration::from_millis(1)))
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("Error in event stream");

    assert!(
        !km_events.is_empty(),
        "{}",
        common::EVENT_STREAM_EMPTY_MESSAGE
    );

    // The following event details correspond to the events in chainflip-eth-contracts/scripts/deploy_and.py
    // See if the key change event matches 1 of the 3 events in the 'deploy_and.py' script
    // All the key strings in this test are decimal pub keys derived from the priv keys in the consts.py script
    // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/consts.py

    km_events
            .iter()
            .find(|event| match &event.event_enum {
            KeyManagerEvent::AggKeySetByAggKey {
                old_key, new_key
            } => {
                assert_eq!(old_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                assert_eq!(new_key,&ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                true
            },
            _ => false,
        }).expect("Didn't find AggKeySetByAggKey event");

    km_events
        .iter()
        .find(|event| match &event.event_enum {
            KeyManagerEvent::AggKeySetByGovKey {
                old_key, new_key
            } => {
                assert_eq!(old_key,&ChainflipKey::from_dec_str("10521316663921629387264629518161886172223783929820773409615991397525613232925",true).unwrap());
                assert_eq!(new_key,&ChainflipKey::from_dec_str("22479114112312168431982914496826057754130808976066989807481484372215659188398",true).unwrap());
                true
            },
            _ => false,
        }).expect("Didn't find AggKeySetByGovKey event");

    km_events
        .iter()
        .find(|event| match &event.event_enum {
            KeyManagerEvent::GovKeySetByGovKey {
                old_key, new_key
            } => {
                assert_eq!(old_key,&ChainflipKey::from_dec_str("29963508097954364125322164523090632495724997135004046323041274775773196467672",true).unwrap());
                assert_eq!(new_key,&ChainflipKey::from_dec_str("35388971693871284788334991319340319470612669764652701045908837459480931993848",false).unwrap());
                true
            },
            _ => false,
        }).expect("Didn't find GovKeySetByGovKey event");
}
