use strum_macros::{EnumIter, EnumString, ToString};

pub struct Coin {
    pub name: String,
    pub symbol: CoinSymbol,
    pub decimals: u32,
    pub requires_return_address: bool,
}

#[derive(Debug, Copy, Clone, EnumString, ToString, EnumIter)]
pub enum CoinSymbol {
    ETH,
    LOKI,
}

impl CoinSymbol {
    pub fn get_coin(&self) -> Coin {
        match self {
            CoinSymbol::LOKI => Coin {
                name: String::from("Loki Network"),
                symbol: (*self),
                decimals: 8,
                requires_return_address: true,
            },
            CoinSymbol::ETH => Coin {
                name: String::from("Ethereum"),
                symbol: (*self),
                decimals: 18,
                requires_return_address: true,
            },
        }
    }
}
