//! This tests integration with the StakeManager contract
//! In order for these tests to work, setup must be completed first

use std::{
    io::{BufRead, BufReader, Error, ErrorKind},
    process::{Command, Stdio},
};

use anyhow::Result;

#[test]
fn setup() -> anyhow::Result<()> {
    let _ = env_logger::init();
    println!("Pull the latest chainflip-eth-contracts repo");

    // TODO: If the directory already exists, delete it.

    let status_clone_eth = Command::new("sh")
        .arg("-c")
        .arg("git clone https://github.com/chainflip-io/chainflip-eth-contracts.git eth-contracts/")
        .status()
        .expect("Could not clone eth repo");

    println!("status: {:#?}", status_clone_eth);

    let brownie_setup_and_test = Command::new("sh")
        .arg("-c")
        .arg("cd eth-contracts && poetry shell && poetry install && brownie pm install OpenZeppelin/openzeppelin-contracts@4.0.0 && brownie test tests/unit/stakeManager/test_registerClaim.py")
        .status();
    // .arg("poetry shell")
    // .arg("poetry install")
    // .arg("brownie pm install OpenZeppelin/openzeppelin-contracts@4.0.0")
    // .arg("brownie test tests/unit/stakeManager/test_registerClaim.py")
    // .stdout(Stdio::piped())
    // .status();

    // .ok_or_else(|| {
    //     Error::new(
    //         ErrorKind::Other,
    //         "Could not run all these setup brownie commands :sad_face:",
    //     )
    // })?;

    println!("brownie setup and test run");

    // let reader = BufReader::new(brownie_setup_and_test);

    // reader
    //     .lines()
    //     .filter_map(|line| line.ok())
    //     .for_each(|line| println!("{}", line));

    println!("Here's the brownie output: {:#?}", brownie_setup_and_test);

    Ok(())
}

// Only test particular tests from the brownie test suite
