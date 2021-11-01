use std::convert::TryInto;

use chainflip_engine::state_chain::client::connect_to_state_chain;
use settings::{CLICommandLineOptions, CLISettings};
use structopt::StructOpt;

use crate::settings::CFCommand::*;
use anyhow::Result;

mod settings;

#[tokio::main]
async fn main() {
    let command_line_opts = CLICommandLineOptions::from_args();
    let cli_settings =
        CLISettings::new(command_line_opts.clone()).expect("Should be able to create settings");

    println!(
        "Connecting to state chain node at: `{}` and using private key located at: `{}`",
        cli_settings.state_chain.ws_endpoint, cli_settings.state_chain.signing_key_file
    );

    let logger = chainflip_engine::logging::utils::new_cli_logger();

    match command_line_opts.cmd {
        Claim {
            amount,
            eth_address,
        } => {
            send_claim_request(
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

async fn send_claim_request(
    amount: u128,
    eth_address: [u8; 20],
    settings: &CLISettings,
    logger: &slog::Logger,
) {
    let (state_chain_client, _) = connect_to_state_chain(&settings.state_chain)
        .await
        .expect("Could not connect to State Chain node");

    let claim_call = pallet_cf_staking::Call::claim(amount, eth_address);

    println!(
        "Submitting claim with amount `{}` and eth-address `0x{}`",
        amount,
        hex::encode(eth_address)
    );

    confirm_submit();

    state_chain_client
        .submit_extrinsic(&logger, claim_call)
        .await
        .expect("Could not submit extrinsic");
}

fn confirm_submit() {
    use std::io;
    use std::io::*;

    print!("Do you wish to proceed? [y/n] > ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("error: failed to get user input");

    let input = input.trim_end_matches(char::is_whitespace);

    match input {
        "y" | "yes" | "1" | "true" | "ofc" => {
            return;
        }
        "n" | "no" | "0" | "false" | "nah" => {
            println!("Ok, exiting.");
            std::process::exit(0);
        }
        _ => {
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
        let input = "0xZ29aB9EbDb421CE48b70699758a6e9a3DBD609C5".to_string();
        assert!(clean_eth_address(input).is_err());

        // success with 0x
        let input = "0xB29aB9EbDb421CE48b70699758a6e9a3DBD609C5".to_string();
        assert!(clean_eth_address(input).is_ok());

        // success without 0x
        let input = "B29aB9EbDb421CE48b70699758a6e9a3DBD609C5".to_string();
        assert!(clean_eth_address(input).is_ok());
    }
}
