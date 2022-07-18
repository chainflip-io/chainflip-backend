#[cfg(test)]
pub mod test_utils {
    use core::time::Duration;

    use tokio::sync::mpsc::UnboundedReceiver;

    const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

    #[cfg(test)]
    async fn recv_with_timeout<I>(receiver: &mut UnboundedReceiver<I>) -> Option<I> {
        tokio::time::timeout(CHANNEL_TIMEOUT, receiver.recv())
            .await
            .ok()?
    }

    #[cfg(test)]
    pub async fn expect_recv_with_timeout<Item: std::fmt::Debug>(
        receiver: &mut UnboundedReceiver<Item>,
    ) -> Item {
        match recv_with_timeout(receiver).await {
            Some(i) => i,
            None => panic!(
                "Timeout waiting for message, expected {}",
                std::any::type_name::<Item>()
            ),
        }
    }
}
