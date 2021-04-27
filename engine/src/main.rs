use crate::witness::sc::sc_observer;

mod mq;
mod p2p;
mod witness;

#[tokio::main]
async fn main() {
    println!("Hello from the CFE!");

    // start observing the state chain and witnessing other chains
    sc_observer::start();
}
