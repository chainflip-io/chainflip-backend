
use crossbeam_channel::Receiver;
use crate::common::Block;

// Connects to lokid can pushes tx to the witness
pub struct LokiConnection {}

impl LokiConnection {

    pub fn new() -> LokiConnection {

        LokiConnection {}

    }

    pub fn start(self) -> Receiver<Block> {

        let (tx, rx) = crossbeam_channel::unbounded::<Block>();

        std::thread::spawn(move || {

            loop {

                // For now we create fake block;
    
                let b = Block { txs: vec![] };
    
                println!("Loki connection: {:?}", &b);
    
                if tx.send(b).is_err() {
                    eprintln!("Could not send block");
                }
    
                std::thread::sleep(std::time::Duration::from_millis(2000));
            }

        });

        rx
    }

}