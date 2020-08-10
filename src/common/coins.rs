use strum_macros::{EnumIter, EnumString, ToString};

pub struct CoinInfo {
    pub name: &'static str,
    pub symbol: Coin,
    pub decimals: u32,
    pub requires_return_address: bool,
}

#[derive(Debug, EnumString, ToString, EnumIter)]
pub enum Coin {
    ETH,
    LOKI,
}

impl Coin {
    pub fn get_info(&self) -> CoinInfo {
        match self {
            Coin::LOKI => CoinInfo {
                name: "Loki Network",
                symbol: Coin::LOKI,
                decimals: 9,
                requires_return_address: true,
            },
            Coin::ETH => CoinInfo {
                name: "Ethereum",
                symbol: Coin::ETH,
                decimals: 18,
                requires_return_address: true,
            },
        }
    }

    pub fn get_decimals(&self) -> u32 {
        self.get_info().decimals
    }
}
