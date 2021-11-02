use std::{convert::TryInto, sync::Arc};

use chainflip_engine::{common::Mutex, state_chain::client::connect_to_state_chain};
use futures::StreamExt;
use settings::{CLICommandLineOptions, CLISettings};
use structopt::StructOpt;

use crate::settings::CFCommand::*;
use anyhow::Result;

mod settings;

#[tokio::main]
async fn main() {
    let command_line_opts = CLICommandLineOptions::from_args();
    let cli_settings = CLISettings::new(command_line_opts.clone()).expect("Could not read config");

    println!(
        "Connecting to state chain node at: `{}` and using private key located at: `{}`",
        cli_settings.state_chain.ws_endpoint, cli_settings.state_chain.signing_key_file
    );

    let logger = chainflip_engine::logging::utils::new_discard_logger();

    match command_line_opts.cmd {
        Claim {
            amount,
            eth_address,
        } => {
            send_claim(
                amount,
                clean_eth_address(eth_address).expect("Invalid ETH address"),
                &cli_settings,
                &logger,
            )
            .await;
        }
    };
}

fn clean_eth_address(dirty_eth_address: String) -> Result<[u8; 20]> {
    let eth_address_hex_str = if dirty_eth_address.starts_with("0x") {
        &dirty_eth_address[2..]
    } else {
        &dirty_eth_address
    };

    let eth_address: [u8; 20] = hex::decode(eth_address_hex_str)?
        .try_into()
        .map_err(|_| anyhow::Error::msg("Could not create a [u8; 20]"))?;

    Ok(eth_address)
}

async fn send_claim(
    amount: u128,
    eth_address: [u8; 20],
    settings: &CLISettings,
    logger: &slog::Logger,
) {
    let (state_chain_client, block_stream) = connect_to_state_chain(&settings.state_chain)
        .await
        .expect("Could not connect to State Chain node");

    println!(
        "Submitting claim with amount `{}` to ETH address `0x{}`",
        amount,
        hex::encode(eth_address)
    );

    confirm_submit();

    // Currently you have to redeem rewards before you can claim them - this may eventually be
    // wrapped into the claim call: https://github.com/chainflip-io/chainflip-backend/issues/769
    let _tx_hash_redeem = state_chain_client
        .submit_extrinsic(&logger, pallet_cf_rewards::Call::redeem_rewards())
        .await
        .expect("Failed to submit redeem extrinsic");

    let tx_hash = state_chain_client
        .submit_extrinsic(&logger, pallet_cf_staking::Call::claim(amount, eth_address))
        .await
        .expect("Could not submit extrinsic");

    println!(
        "Your claim has transaction hash: `{:?}`. Waiting for your request to be confirmed...",
        tx_hash
    );

    let block_stream = Arc::new(Mutex::new(block_stream));

    let events = state_chain_client
        .watch_submitted_extrinsic(tx_hash, block_stream.clone())
        .await
        .expect("Failed to watch extrinsic");

    for event in events {
        if let state_chain_runtime::Event::pallet_cf_threshold_signature_Instance0(
            pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(_, ..),
        ) = event
        {
            println!("Your claim request is on chain.\nWaiting for signed claim data...");
            'outer: while let Some(block_header) = block_stream.lock().await.next().await {
                let header = block_header.expect("Failed to get a valid block header");
                let events = state_chain_client.events(&header).await.expect(&format!(
                    "Failed to fetch events for block: {}",
                    header.number
                ));
                for (_phase, event, _) in events {
                    match event {
                        state_chain_runtime::Event::pallet_cf_staking(
                            pallet_cf_staking::Event::ClaimSignatureIssued(
                                validator_id,
                                signed_payload,
                            ),
                        ) => {
                            if validator_id == state_chain_client.our_account_id {
                                println!("Here's the signed claim data. Please proceed to the Staking UI to complete your claim. <LINK>");
                                println!("\n{}\n", hex::encode(signed_payload));
                                break 'outer;
                            }
                        }
                        _ => {
                            // ignore
                        }
                    }
                }
            }
        }
    }
}

fn confirm_submit() {
    use std::io;
    use std::io::*;

    print!("Do you wish to proceed? [y/n] > ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Error: Failed to get user input");

    let input = input.trim_end_matches(char::is_whitespace);

    match input {
        "y" | "yes" | "1" | "true" | "ofc" => {
            println!("Submitting...");
            return;
        }
        "n" | "no" | "0" | "false" | "nah" => {
            println!("Ok, exiting...");
            std::process::exit(0);
        }
        _ => {
            eprintln!("Invalid: please type `y` or `n` to confirm");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_eth_address() {
        // fail too short
        let input = "0x323232".to_string();
        assert!(clean_eth_address(input).is_err());

        // fail invalid chars
        let input = "0xZ29aB9EbDb421CE48b70flippya6e9a3DBD609C5".to_string();
        assert!(clean_eth_address(input).is_err());

        // success with 0x
        let input = "0xB29aB9EbDb421CE48b70699758a6e9a3DBD609C5".to_string();
        assert!(clean_eth_address(input).is_ok());

        // success without 0x
        let input = "B29aB9EbDb421CE48b70699758a6e9a3DBD609C5".to_string();
        assert!(clean_eth_address(input).is_ok());
    }
}
