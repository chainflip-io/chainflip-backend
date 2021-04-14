use super::{IMQClient, MQError, Message, Options, Result};
use crossbeam_channel::Receiver;
use nats;

// This will likely have a private field containing the underlying mq client
pub struct NatsMQClient {
    /// The nats.rs Connection to the Nats server
    conn: nats::Connection,
}

impl IMQClient<Message> for NatsMQClient {
    fn connect(opts: Options) -> Self {
        let conn = nats::connect(opts.url).expect("Could not connect to Nats");
        NatsMQClient { conn }
    }

    fn publish(&self, subject: &str, message: Vec<u8>) -> Result<()> {
        println!("Publish message: {:#?}, to subject: {}", message, subject);
        self.conn
            .publish(subject, message)
            .map_err(|err| MQError::PublishError(err))
    }

    fn subscribe(&self, subject: &str) -> Result<Receiver<Message>> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn setup_client() -> NatsMQClient {
        let options = Options {
            url: "http://localhost:4222",
        };

        NatsMQClient::connect(options)
    }

    #[ignore = "Depends on Nats being online"]
    #[test]
    fn connect_to_nats() {
        let nats_client = setup_client();
        let client_ip = nats_client.conn.client_ip();
        assert!(client_ip.is_ok())
    }

    #[test]
    fn publish_to_subject() {
        let nats_client = setup_client();
        nats_client.publish("witness.eth", "hello".as_bytes().to_owned())
    }
}
