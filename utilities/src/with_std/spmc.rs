use futures::stream::{Stream, StreamExt};

pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
	let (sender, receiver) = async_broadcast::broadcast(capacity);
	let (detect_close_sender, detect_close_receiver) = tokio::sync::watch::channel(());

	(Sender(sender, detect_close_sender), Receiver(receiver, detect_close_receiver))
}

pub struct Sender<T>(async_broadcast::Sender<T>, tokio::sync::watch::Sender<()>);

impl<T: Clone> Sender<T> {
	/// Sends an item to all receivers
	pub async fn send(&self, t: T) -> Result<(), async_broadcast::SendError<T>> {
		self.0
			.broadcast(t)
			.await
			.map(|option| assert!(option.is_none(), "async_broadcast overflow is off"))
	}
}
impl<T> Sender<T> {
	/// Creates a receiver, and reopens channel if it was previously closed
	pub fn receiver(&mut self) -> Receiver<T> {
		Receiver(
			{
				let receiver = self.0.new_receiver();

				if receiver.is_closed() {
					let (new_sender, new_receiver) = async_broadcast::broadcast(self.0.capacity());
					let _ = std::mem::replace(&mut self.0, new_sender);
					new_receiver
				} else {
					receiver
				}
			},
			self.1.subscribe(), /* Will reopen channel even if closed previously */
		)
	}

	/// Waits until all receivers have been dropped, and therefore the channel is closed
	pub async fn closed(&self) {
		self.1.closed().await
	}
}

#[derive(Clone)]
pub struct Receiver<T>(async_broadcast::Receiver<T>, tokio::sync::watch::Receiver<()>);
impl<T: Clone> Stream for Receiver<T> {
	type Item = T;

	fn poll_next(
		mut self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Option<Self::Item>> {
		self.0.poll_next_unpin(cx)
	}
}

#[cfg(test)]
mod test {
	use futures::{future::join, FutureExt};

	use super::*;

	#[tokio::test]
	async fn channel_allows_reconnection() {
		let (mut sender, receiver) = channel(2);
		drop(receiver);
		assert!(matches!(sender.send(1).await, Err(_)));
		let mut receiver = sender.receiver();
		sender.send(1).await.unwrap();
		sender.send(1).await.unwrap();
		drop(sender);
		assert_eq!(receiver.next().await, Some(1));
		assert_eq!(receiver.next().await, Some(1));
		assert_eq!(receiver.next().await, None);
	}

	#[tokio::test]
	async fn broadcasts() {
		let (mut sender, mut receiver_1) = channel(1);
		let mut receiver_2 = sender.receiver();
		let mut receiver_3 = receiver_1.clone();

		sender.send(1).await.unwrap();

		assert_eq!(receiver_1.next().await, Some(1));
		assert_eq!(receiver_2.next().await, Some(1));
		assert_eq!(receiver_3.next().await, Some(1));

		drop(sender);

		assert_eq!(receiver_1.next().await, None);
		assert_eq!(receiver_2.next().await, None);
		assert_eq!(receiver_3.next().await, None);
	}

	#[tokio::test]
	async fn waiting_for_closed() {
		let (mut sender, mut receiver_1) = channel(1);
		let receiver_2 = sender.receiver();
		let receiver_3 = receiver_1.clone();

		assert!(sender.closed().now_or_never().is_none());

		sender.send(1).await.unwrap();

		join(
			async move {
				drop(receiver_2);
				assert_eq!(receiver_1.next().await, Some(1));
				drop(receiver_1);
				drop(receiver_3);
			},
			sender.closed(),
		)
		.await;
	}
}
