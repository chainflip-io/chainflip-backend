pub mod http_rpc;
pub mod retry_rpc;
pub mod rpc;

pub type PolkadotHash = <subxt::PolkadotConfig as subxt::Config>::Hash;
pub type PolkadotHeader = <subxt::PolkadotConfig as subxt::Config>::Header;
