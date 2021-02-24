use crate::types::{addresses::EthereumAddress, Network};
use sp_std::if_std;

// Deposit ETH contract init code: 6080604052348015600f57600080fd5b5033fffe
pub const ETH_DEPOSIT_INIT_CODE: [u8; 20] = [
    96, 128, 96, 64, 82, 52, 128, 21, 96, 15, 87, 96, 0, 128, 253, 91, 80, 51, 255, 254,
];

/// Get the contract address for the vault on the given network
pub fn get_vault_address(_network: Network) -> EthereumAddress {
    // Temporarily just use 'null' address until we have deployed the vault contract
    EthereumAddress([0; 20])
}

// Need to use if_std here because `include_bytes!` is a std only macro
if_std! {
    lazy_static! {
        pub static ref ETH_VAULT_CONTRACT_ABI: Vec<u8> = {
            include_bytes!("../contracts/Vault.json").to_vec()
        };
    }
}
