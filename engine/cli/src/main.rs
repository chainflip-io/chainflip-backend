use chainflip_engine::state_chain::client::connect_to_state_chain;
use futures::StreamExt;
use settings::{CLICommandLineOptions, CLISettings};
use state_chain_node::chain_spec::get_from_seed;
use state_chain_runtime::opaque::SessionKeys;
use structopt::StructOpt;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_finality_grandpa::AuthorityId as GrandpaId;

use crate::settings::CFCommand::*;
use anyhow::Result;
use utilities::clean_eth_address;

mod settings;

#[tokio::main]
async fn main() {
    std::process::exit(match run_cli().await {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("Error: {:?}", err);
            1
        }
    })
}

async fn run_cli() -> Result<()> {
    let command_line_opts = CLICommandLineOptions::from_args();
    let cli_settings = CLISettings::new(command_line_opts.clone()).map_err(|_| anyhow::Error::msg("Please ensure your config file path is configured correctly. Or set all required command line arguments."))?;

    println!(
        "Connecting to state chain node at: `{}` and using private key located at: `{}`",
        cli_settings.state_chain.ws_endpoint,
        cli_settings.state_chain.signing_key_file.display()
    );

    let logger = chainflip_engine::logging::utils::new_discard_logger();

    match command_line_opts.cmd {
        Claim {
            amount,
            eth_address,
        } => Ok(send_claim(
            amount,
            clean_eth_address(&eth_address)
                .map_err(|_| anyhow::Error::msg("You supplied an invalid ETH address"))?,
            &cli_settings,
            &logger,
        )
        .await?),
        Rotate {} => Ok(rotate_keys(&cli_settings, &logger).await?),
    }
}

async fn send_claim(
    amount: f64,
    eth_address: [u8; 20],
    settings: &CLISettings,
    logger: &slog::Logger,
) -> Result<()> {
    let atomic_amount: u128 = (amount * 10_f64.powi(18)) as u128;

    println!(
        "Submitting claim with amount `{}` FLIP (`{}` Flipperinos) to ETH address `0x{}`",
        amount,
        atomic_amount,
        hex::encode(eth_address)
    );

    if !confirm_submit() {
        return Ok(());
    }

    let (_, block_stream, state_chain_client) = connect_to_state_chain(&settings.state_chain).await.map_err(|_| anyhow::Error::msg("Failed to connect to state chain node. Please ensure your state_chain_ws_endpoint is pointing to a working node."))?;

    // Currently you have to redeem rewards before you can claim them - this may eventually be
    // wrapped into the claim call: https://github.com/chainflip-io/chainflip-backend/issues/769
    let _tx_hash_redeem = state_chain_client
        .submit_signed_extrinsic(logger, pallet_cf_rewards::Call::redeem_rewards())
        .await
        .expect("Failed to submit redeem extrinsic");

    let tx_hash = state_chain_client
        .submit_signed_extrinsic(
            logger,
            pallet_cf_staking::Call::claim(atomic_amount, eth_address),
        )
        .await
        .expect("Failed to submit claim extrinsic");

    println!(
        "Your claim has transaction hash: `{:#x}`. Waiting for your request to be confirmed...",
        tx_hash
    );

    let mut block_stream = Box::new(block_stream);
    let block_stream = block_stream.as_mut();

    let events = state_chain_client
        .watch_submitted_extrinsic(tx_hash, block_stream)
        .await
        .expect("Failed to watch extrinsic");

    for event in events {
        if let state_chain_runtime::Event::EthereumThresholdSigner(
            pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(..),
        ) = event
        {
            println!("Your claim request is on chain.\nWaiting for signed claim data...");
            'outer: while let Some(result_header) = block_stream.next().await {
                let header = result_header.expect("Failed to get a valid block header");
                let events = state_chain_client
                    .get_events(header.hash())
                    .await
                    .unwrap_or_else(|e| {
                        panic!("Failed to fetch events for block: {}, {}", header.number, e)
                    });
                for (_phase, event, _) in events {
                    if let state_chain_runtime::Event::Staking(
                        pallet_cf_staking::Event::ClaimSignatureIssued(validator_id, _),
                    ) = event
                    {
                        if validator_id == state_chain_client.our_account_id {
                            println!("Your claim request has been successfully registered. Please proceed to the Staking UI to complete your claim. <LINK>");
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn rotate_keys(
    settings: &CLISettings,
    logger: &slog::Logger
) -> Result<()> {
    let (_, _, state_chain_client) = connect_to_state_chain(&settings.state_chain).await.map_err(|e| anyhow::Error::msg(format!("{:?} Failed to connect to state chain node. Please ensure your state_chain_ws_endpoint is pointing to a working node.", e)))?;
    let seed = state_chain_client
        .rotate_session_keys()
        .await
        .unwrap();
    println!("New session key {:?}", seed);

    let new_session_key = SessionKeys {
        aura: get_from_seed::<AuraId>(&seed),
        grandpa: get_from_seed::<GrandpaId>(&seed),
    };

    let tx_hash = state_chain_client
        .submit_signed_extrinsic(
            logger,
            pallet_cf_validator::Call::set_keys(new_session_key, [0; 8].to_vec())
        )
    .await
    .expect("Failed to submit set_keys extrinsic");

    println!("Session key rotated at tx {:?}.", tx_hash);

    Ok(())
}

fn confirm_submit() -> bool {
    use std::io;
    use std::io::*;

    loop {
        print!("Do you wish to proceed? [y/n] > ");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Error: Failed to get user input");

        let input = input.trim();

        match input {
            "y" | "yes" | "1" | "true" | "ofc" => {
                println!("Submitting...");
                return true;
            }
            "n" | "no" | "0" | "false" | "nah" => {
                println!("Ok, exiting...");
                return false;
            }
            _ => {
                continue;
            }
        }
    }
}
