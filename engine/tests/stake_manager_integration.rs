//! This tests integration with the StakeManager contract
//! In order for these tests to work, nats and ganache with the preloaded db
//! in `./eth-db` must be loaded in
use std::{str::FromStr, time::Duration};

use chainflip_engine::{
    eth::{self, stake_manager::stake_manager::StakeManagerEvent},
    mq::{nats_client::NatsMQClient, pin_message_stream, IMQClient, Subject},
    settings::Settings,
};

use sp_runtime::AccountId32;
use tokio_stream::StreamExt;

use web3::types::U256;

use slog::{o, Drain};

#[tokio::test]
pub async fn test_all_stake_manager_events() {
    struct TestEvents {
        staked: bool,
        claim_registered: bool,
        claim_executed: bool,
        min_stake_changed: bool,
        flip_supply_updated: bool,
    }
    let mut events_check_list = TestEvents {
        staked: false,
        claim_registered: false,
        claim_executed: false,
        min_stake_changed: false,
        flip_supply_updated: false,
    };

    let drain = slog_json::Json::new(std::io::stdout())
        .add_default_keys()
        .build()
        .fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let root_logger = slog::Logger::root(drain, o!());

    let settings = Settings::from_file("config/Testing.toml").unwrap();
    let mq_c = NatsMQClient::new(&settings.message_queue).await.unwrap();

    // subscribe before pushing events to the queue
    let sm_event_stream = mq_c
        .subscribe::<StakeManagerEvent>(Subject::StakeManager)
        .await
        .unwrap();

    // The Stake Manager Witness will run forever unless we stop it after a short time
    // in which it should have already done it's job.
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        eth::stake_manager::start_stake_manager_witness(&settings, mq_c, &root_logger),
    )
    .await;
    slog::info!(&root_logger, "Subscribed");

    // The following events correspond to the events in chainflip-eth-contracts/scripts/deploy_and.py
    let mut sm_event_stream = pin_message_stream(sm_event_stream);
    loop {
        // All events should already be built up in the event stream, so no need to wait.
        match tokio::time::timeout(Duration::from_millis(1), sm_event_stream.next()).await {
            Ok(Some(Ok(StakeManagerEvent::Staked {
                account_id,
                amount,
                return_addr,
                ..
            }))) => {
                assert_eq!(
                    account_id,
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(amount, 40000000000000000000000);
                assert_eq!(
                    return_addr,
                    web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
                        .unwrap()
                );
                events_check_list.staked = true;
            }

            Ok(Some(Ok(StakeManagerEvent::ClaimRegistered {
                account_id,
                amount,
                staker,
                ..
            }))) => {
                assert_eq!(
                    account_id,
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(
                    amount,
                    U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(
                    staker,
                    web3::types::H160::from_str("0x33a4622b82d4c04a53e170c638b944ce27cffce3")
                        .unwrap()
                );
                events_check_list.claim_registered = true;
            }

            Ok(Some(Ok(StakeManagerEvent::ClaimExecuted {
                account_id, amount, ..
            }))) => {
                assert_eq!(
                    account_id,
                    AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(amount, 13333333333333334032384);
                events_check_list.claim_executed = true;
            }

            Ok(Some(Ok(StakeManagerEvent::MinStakeChanged {
                old_min_stake,
                new_min_stake,
                ..
            }))) => {
                assert_eq!(
                    old_min_stake,
                    U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_min_stake,
                    U256::from_dec_str("13333333333333334032384").unwrap()
                );
                events_check_list.min_stake_changed = true;
            }

            Ok(Some(Ok(StakeManagerEvent::FlipSupplyUpdated {
                old_supply,
                new_supply,
                ..
            }))) => {
                assert_eq!(
                    old_supply,
                    U256::from_dec_str("90000000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_supply,
                    U256::from_dec_str("100000000000000000000000000").unwrap()
                );
                events_check_list.flip_supply_updated = true;
            }

            Ok(_) => {
                panic!("Error in event stream")
            }

            Err(_) => {
                // Timeout, all events in the stream have been check.
                break;
            }
        }
    }

    if !events_check_list.staked
        && !events_check_list.claim_registered
        && !events_check_list.claim_executed
        && !events_check_list.min_stake_changed
        && !events_check_list.flip_supply_updated
    {
        panic!("Event stream was empty. Have you ran the setup script to deploy/run the contracts?")
    }

    assert_eq!(events_check_list.staked, true, "Staked event was not seen");
    assert_eq!(
        events_check_list.claim_registered, true,
        "ClaimRegistered event was not seen"
    );
    assert_eq!(
        events_check_list.claim_executed, true,
        "ClaimExecuted event was not seen"
    );
    assert_eq!(
        events_check_list.min_stake_changed, true,
        "MinStakeChanged event was not seen"
    );
    assert_eq!(
        events_check_list.flip_supply_updated, true,
        "FlipSupplyUpdated event was not seen"
    );
}
