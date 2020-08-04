use crate::common::{Timestamp, WalletAddress};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuoteId(u64);

impl QuoteId {

    pub fn new(id : u64) -> Self {
        QuoteId {0: id}
    }
}

#[derive(Debug, Clone)]
pub struct QuoteTx {
    pub id: QuoteId,
    pub timestamp: Timestamp,
    pub deposit_address: WalletAddress,
    pub return_address: WalletAddress,
    // There are more fields, but I will add them
    // when I have actually start using them
}

#[derive(Debug)]
pub struct CoinTx {

    pub id: u64,
    pub timestamp: Timestamp,
    pub deposit_address: WalletAddress,
    pub return_address: WalletAddress,
}

#[derive(Debug, Clone, Copy)]
pub struct WitnessTx {
    pub quote_id: QuoteId
}

impl WitnessTx {

    pub fn new(id: QuoteId) -> Self {
        WitnessTx { quote_id: id }
    }

}