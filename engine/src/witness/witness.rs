use super::sc::{self, sc_observer};

pub async fn main() {
    // Start the state chain witness
    sc_observer::start().await;

    // Start the other witness processes...
}
