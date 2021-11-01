use std::convert::TryInto;

use chainflip_engine::{settings::StateChain, state_chain::client::connect_to_state_chain};
use clap::{App, Arg, SubCommand};

use anyhow::Result;

// The commands:
const CLAIM: &str = "claim";

#[tokio::main]
async fn main() {
    let matches = App::new("Chainflip CLI")
        .version("0.1.0")
        .author("Chainflip Team <team@chainflip.io>")
        .about("Run commands and use Chainflip")
        .arg(
            Arg::with_name("CFE config")
                .short("c")
                .long("config")
                .help("Points to the configuration of the Chainflip engine")
                .required(false),
        )
        .subcommand(
            SubCommand::with_name(CLAIM)
                .arg(
                    Arg::with_name("amount")
                        .help("Amount of FLIP to claim")
                        .required(true),
                )
                .arg(
                    Arg::with_name("eth_address")
                        .help("ETH address claimed FLIP will be sent to")
                        .required(true),
                ),
        )
        .about("register for a claim certificate")
        .get_matches();

    match matches.subcommand_matches(CLAIM) {
        Some(args) => {
            let amount: u128 =
                str::parse::<u128>(args.value_of("amount").expect("amount is required"))
                    .expect("Invalid amount provided");

            let eth_address_arg = args
                .value_of("eth_address")
                .expect("Ethereum return address not provided");

            let eth_address_arg = clean_eth_address(eth_address_arg).expect("Invalid ETH address");

            send_claim_request(amount, eth_address_arg).await;
        }
        _ => (),
    }
}

fn clean_eth_address(dirty_eth_address: &str) -> Result<[u8; 20]> {
    let eth_address_hex_str = if dirty_eth_address.starts_with("0x") {
        &dirty_eth_address[2..]
    } else {
        dirty_eth_address
    };

    let eth_address: [u8; 20] = hex::decode(eth_address_hex_str)?
        .try_into()
        .map_err(|_| anyhow::Error::msg("Could not create a [u8; 20]"))?;

    Ok(eth_address)
}

async fn send_claim_request(amount: u128, eth_address: [u8; 20]) {
    // TODO: Read in actual values here. Take as CLI args, and use these as a default
    let state_chain_settings = StateChain {
        ws_endpoint: "ws://127.0.0.1:9944".to_string(),
        signing_key_file: "/Users/kaz/Documents/cf-dev-keys/bashful_key".to_string(),
    };

    let logger = chainflip_engine::logging::utils::new_cli_logger();

    let (state_chain_client, _) = connect_to_state_chain(&state_chain_settings)
        .await
        .expect("Could not connect to State Chain node");

    let claim_call = pallet_cf_staking::Call::claim(amount, eth_address);

    state_chain_client
        .submit_extrinsic(&logger, claim_call)
        .await
        .expect("Could not submit extrinsic");
}
