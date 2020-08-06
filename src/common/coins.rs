use strum_macros::{EnumIter, EnumString, ToString};

pub struct CoinInfo {
    pub name: String,
    pub symbol: Coin,
    pub decimals: u32,
    pub requires_return_address: bool,
}

#[derive(Debug, Copy, Clone, EnumString, ToString, EnumIter)]
pub enum Coin {
    ETH,
    LOKI,
}

impl Coin {
    pub fn get_info(&self) -> CoinInfo {
        match self {
            Coin::LOKI => CoinInfo {
                name: String::from("Loki Network"),
                symbol: (*self),
                decimals: 8,
                requires_return_address: true,
            },
            Coin::ETH => CoinInfo {
                name: String::from("Ethereum"),
                symbol: (*self),
                decimals: 18,
                requires_return_address: true,
            },
        }
    }

    pub fn get_decimals(&self) -> u32 {
        self.get_info().decimals
    }
}
