use super::sc::{self, sc_witness};

pub fn main() {
    // Start the state chain witness
    sc_witness::start();

    // Start the other witness processes
}
