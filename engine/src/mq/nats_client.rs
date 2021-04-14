use super::{IMQClient, MQError, Message, Options, Result};
use crossbeam_channel::Receiver;
use nats;

// This will likely have a private field containing the underlying mq client
pub struct NatsMQClient {
    /// The nats.rs Connection to the Nats server
    conn: nats::Connection,
}

impl From<nats::Message> for Message {
    fn from(msg: nats::Message) -> Self {
        Message(msg.data)
    }
}

impl IMQClient<Message> for NatsMQClient {
    fn connect(opts: Options) -> Self {
        let conn =
            nats::connect(opts.url).expect(&format!("Could not connect to Nats on {}", opts.url));
        NatsMQClient { conn }
    }

    fn publish(&self, subject: &str, message: Vec<u8>) -> Result<()> {
        self.conn
            .publish(subject, message)
            .map_err(|_| MQError::PublishError)
    }

    fn subscribe(&self, subject: &str) -> Result<Receiver<Message>> {
        let subscription = self
            .conn
            .subscribe(subject)
            .map_err(|_| MQError::SubscribeError)?;

        let sub_recv = subscription.receiver();

        // Create new channel with the general Message type
        let (send, receiver) = crossbeam_channel::unbounded::<Message>();

        for m in sub_recv.try_recv() {
            send.send(m.into()).map_err(|_| MQError::ConversionError)?;
        }

        Ok(receiver)
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

    #[ignore = "Depends on Nats being online"]
    #[test]
    fn publish_to_subject() {
        let nats_client = setup_client();
        let res = nats_client.publish("witness.eth", "hello".as_bytes().to_owned());
        assert!(res.is_ok());
    }

    #[ignore = "Depends on Nats being online"]
    #[test]
    fn subscribe_to_eth_witness() {
        let nats_client = setup_client();
        let receiver = nats_client.subscribe("witness.eth");
        assert!(receiver.is_ok());
        let receiver = receiver.unwrap();

        let test_message = "I SAW A TRANSACTION".as_bytes().to_owned();

        // Publish something to the mq so that we can read it
        let handle = std::thread::spawn(move || {
            let pub_res = nats_client.publish("witness.eth", test_message.clone());
            assert!(pub_res.is_ok());
        });

        let msg_received = receiver.recv_timeout(std::time::Duration::from_secs(5));
        println!("Message received: {:#?}", msg_received);
        // assert_eq!(msg_received.0, test_message);
        handle.join().unwrap();
    }
}
