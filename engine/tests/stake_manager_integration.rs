//! This tests integration with the StakeManager contract
//! In order for these tests to work, nats and ganache with the preloaded db
//! in `./eth-db` must be loaded in
use std::str::FromStr;

use chainflip_engine::{
    eth::{
        new_synced_web3_client,
        stake_manager::{StakeManager, StakeManagerEvent},
    },
    logging::utils,
    settings::Settings,
};

use futures::stream::StreamExt;
use sp_runtime::AccountId32;

use web3::types::U256;

#[tokio::test]
pub async fn test_all_stake_manager_events() {
    let root_logger = utils::create_cli_logger();

    let settings = Settings::from_file("config/Testing.toml").unwrap();

    let web3 = new_synced_web3_client(&settings, &root_logger)
        .await
        .unwrap();

    let stake_manager = StakeManager::new(&settings).unwrap();

    // The stream is infinite unless we stop it after a short time
    // in which it should have already done it's job.
    let sm_events = stake_manager
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
        !sm_events.is_empty(),
        "r#
            Event stream was empty.
            - Have you run the setup script to deploy/run the contracts?
            - Are you pointing to the correct contract address?",
    );

    // The following event details correspond to the events in chainflip-eth-contracts/scripts/deploy_and.py
    sm_events
        .iter()
        .find(|event| match &event.event_enum {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                return_addr,
                ..
            } => {
                assert_eq!(
                    account_id,
                    &AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(amount, &40000000000000000000000);
                assert_eq!(
                    return_addr,
                    &web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
                        .unwrap()
                );
                true
            }
            _ => false,
        })
        .expect("Didn't find the Staked event");

    sm_events
        .iter()
        .find(|event| match &event.event_enum {
            StakeManagerEvent::ClaimRegistered {
                account_id,
                amount,
                staker,
                ..
            } => {
                assert_eq!(
                    account_id,
                    &AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(
                    amount,
                    &U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(
                    staker,
                    &web3::types::H160::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8")
                        .unwrap()
                );
                true
            }
            _ => false,
        })
        .expect("Didn't find the ClaimRegistered event");

    sm_events
        .iter()
        .find(|event| match &event.event_enum {
            StakeManagerEvent::ClaimExecuted {
                account_id, amount, ..
            } => {
                assert_eq!(
                    account_id,
                    &AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(amount, &13333333333333334032384);
                true
            }
            _ => false,
        })
        .expect("Didn't find the ClaimExecuted event");

    sm_events
        .iter()
        .find(|event| match &event.event_enum {
            StakeManagerEvent::MinStakeChanged {
                old_min_stake,
                new_min_stake,
                ..
            } => {
                assert_eq!(
                    old_min_stake,
                    &U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_min_stake,
                    &U256::from_dec_str("13333333333333334032384").unwrap()
                );
                true
            }
            _ => false,
        })
        .expect("Didn't find the MinStakeChanged event");

    sm_events
        .iter()
        .find(|event| match &event.event_enum {
            StakeManagerEvent::FlipSupplyUpdated {
                old_supply,
                new_supply,
                ..
            } => {
                assert_eq!(
                    old_supply,
                    &U256::from_dec_str("90000000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_supply,
                    &U256::from_dec_str("100000000000000000000000000").unwrap()
                );
                true
            }
            _ => false,
        })
        .expect("Didn't find the FlipSupplyUpdated event");
}
