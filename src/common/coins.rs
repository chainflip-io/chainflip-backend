use std::fmt;
use std::result::Result;
use std::str;

pub struct Coin {
    pub name: String,
    pub symbol: CoinSymbol,
    pub decimals: u32,
    pub requires_return_address: bool,
}

// TODO: Do we want to create an iterator for this?
#[derive(Debug, Copy, Clone)]
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

impl fmt::Display for CoinSymbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl str::FromStr for CoinSymbol {
    type Err = String;

    fn from_str(string: &str) -> Result<CoinSymbol, String> {
        let symbol = string.to_lowercase();
        match symbol.as_str() {
            "eth" => Ok(CoinSymbol::ETH),
            "loki" => Ok(CoinSymbol::LOKI),
            _ => Err(String::from("Invalid coin symbol!")),
        }
    }
}
