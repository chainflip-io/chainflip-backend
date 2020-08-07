use crate::common::{Timestamp, WalletAddress};
use crate::transactions::{QuoteId, QuoteTx};

pub fn create_fake_quote_tx() -> QuoteTx {
    let return_address = WalletAddress::new("Alice");
    let deposit_address = WalletAddress::new("Bob");
    let timestamp = Timestamp::now();

    let quote = QuoteTx {
        id: QuoteId::new(0),
        timestamp,
        deposit_address,
        return_address,
    };

    quote
}

/// Creates a new random file that gets removed
/// when this object is destructed
pub struct TempRandomFile {
    path: String,
}

impl TempRandomFile {
    pub fn new() -> Self {
        use rand::Rng;

        let rand_filename = format!("temp-{}.db", rand::thread_rng().gen::<u64>());

        TempRandomFile {
            path: rand_filename,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Drop for TempRandomFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path)
            .expect(&format!("Could not remove temp file {}", &self.path));
    }
}
