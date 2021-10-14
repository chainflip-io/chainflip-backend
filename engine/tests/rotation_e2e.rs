use chainflip_engine::{logging::utils::create_cli_logger, settings::Settings, state_chain};
use tokio_stream::StreamExt;

/// Timeout the test if we don't get to the next step in this many blocks
const MAX_TIME_FOR_NEXT_STEP_IN_BLOCKS: i32 = 12;

/// Force a vault rotation and then monitor for expected events
/// on the State Chain and the ethereum contracts
/// This test should be run on a fresh network
#[tokio::test]
pub async fn vault_rotation_end_to_end() {
    let root_logger = create_cli_logger();

    // ensure this is pointing to snow white's settings
    let settings = Settings::from_file("config/SnowWhite.toml")
        .expect("Failed to read settings `config/SnowWhite.toml`");

    let (state_chain_client, mut state_chain_block_stream) =
        state_chain::client::connect_to_state_chain(&settings)
            .await
            .expect("Could not connect to state chain");

    // ====== Kick off the rotation =======

    // propose(sudo(force_rotation))
    // on a new chain, this will be governance call #1
    state_chain_client
        .submit_extrinsic(
            &root_logger,
            pallet_cf_governance::Call::propose_governance_extrinsic(Box::new(
                pallet_cf_governance::Call::call_as_sudo(Box::new(
                    pallet_cf_validator::Call::force_rotation().into(),
                ))
                .into(),
            )),
        )
        .await;

    // approve(1)
    state_chain_client
        .submit_extrinsic(&root_logger, pallet_cf_governance::Call::approve(1))
        .await;

    // execute(1)
    state_chain_client
        .submit_extrinsic(&root_logger, pallet_cf_governance::Call::execute(1))
        .await;

    // ======= Rotation should begin now =======

    // We only care about these events, in this order.
    // 1. KeygenRequest
    // 2. ThresholdSignatureRequest
    // 3. NewEpoch

    // this ensures we receive the events we care about in the correct order
    let mut order_counter = 1;
    // ensure we timeout if we've waited too many blocks without a rotation
    let mut block_counter = 0;

    // now monitor for the events we expect
    // we can state machine through the events
    'block_loop: while let Some(result_block_header) = state_chain_block_stream.next().await {
        let block_header = result_block_header.expect("Should be valid block header");
        match state_chain_client.events(&block_header).await {
            Ok(events) => {
                for (_phase, event, _topics) in events {
                    match event {
                        state_chain_runtime::Event::pallet_cf_vaults(
                            // we don't realllly need to assert over who is in this batch. So leave it for now
                            pallet_cf_vaults::Event::KeygenRequest(ceremony_id, _keygen_request),
                        ) => {
                            slog::info!(
                                root_logger,
                                "KeygenRequest emitted for ceremony_id: {:?}",
                                1
                            );
                            assert_eq!(order_counter, 1);
                            assert_eq!(ceremony_id, 1);
                            order_counter += 1;
                        }
                        state_chain_runtime::Event::pallet_cf_vaults(
                            pallet_cf_vaults::Event::ThresholdSignatureRequest(
                                ceremony_id,
                                _signature_request,
                            ),
                        ) => {
                            slog::info!(
                                root_logger,
                                "Signing event received with ceremony_id: {:?}",
                                ceremony_id
                            );
                            assert_eq!(order_counter, 2);
                            assert_eq!(ceremony_id, 1);
                            order_counter += 1;
                        }
                        state_chain_runtime::Event::pallet_cf_validator(
                            pallet_cf_validator::Event::NewEpoch(epoch_index),
                        ) => {
                            slog::info!(
                                root_logger,
                                "NewEpoch event received, epoch index: {:?}",
                                epoch_index
                            );
                            assert_eq!(order_counter, 3);
                            // if we passed this assert, then we can exit the loop
                            break 'block_loop;
                        }
                        _ => {
                            // events we don't care about
                        }
                    }
                }
            }
            Err(e) => {
                panic!("Error getting events: {:?}", e);
            }
        }
        block_counter += 1;
        if block_counter > MAX_TIME_FOR_NEXT_STEP_IN_BLOCKS {
            panic!(
                "More than {} blocks and still waiting on event #{} there has not been a new epoch.",
                MAX_TIME_FOR_NEXT_STEP_IN_BLOCKS, order_counter
            );
        }
    }
}
