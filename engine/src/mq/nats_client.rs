use super::{IMQClient, MQError, Message, Options, Result};
use crossbeam_channel::Receiver;
use nats;

// This will likely have a private field containing the underlying mq client
pub struct NatsMQClient {
    /// The nats.rs connection to the Nats server
    conn: nats::Connection,
}

impl IMQClient<Message> for NatsMQClient {
    fn connect(opts: Options) -> Self {
        println!("First we try to connect to Nats");
        let conn = nats::connect(opts.url).expect("Could not connect to Nats");
        NatsMQClient { conn }
    }

    fn publish(&self, subject: &str, message: Vec<u8>) {
        todo!()
    }

    fn subscribe(&self, subject: &str) -> Result<Receiver<Message>> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[ignore = "Depends on Nats being online"]
    #[test]
    fn connect_to_nats() {
        let options = Options {
            url: "http://localhost:4222",
        };

        let nats_client = NatsMQClient::connect(options);
        let client_id = nats_client.conn.client_ip();
        assert!(client_id.is_ok())
    }
}
