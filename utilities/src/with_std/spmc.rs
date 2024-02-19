use futures::stream::{Stream, StreamExt};
use tracing::warn;

pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
	let (sender, receiver) = async_broadcast::broadcast(capacity);
	let (detect_close_sender, detect_close_receiver) = tokio::sync::watch::channel(());

	(
		Sender { sender, detect_close_sender },
		Receiver { receiver, _detect_close_receiver: detect_close_receiver },
	)
}

pub struct Sender<T> {
	sender: async_broadcast::Sender<T>,
	detect_close_sender: tokio::sync::watch::Sender<()>,
}

impl<T: Clone> Sender<T> {
	/// Sends an item to all receivers
	#[allow(clippy::manual_async_fn)]
	#[track_caller]
	pub fn send(&self, msg: T) -> impl futures::Future<Output = bool> + '_ {
		async move {
			match self.sender.try_broadcast(msg) {
				Ok(None) => true,
				Ok(Some(_)) => unreachable!("async_broadcast feature unused"),
				Err(error) => match error {
					async_broadcast::TrySendError::Full(msg) => {
						warn!("Waiting for space in channel which is currently full with a capacity of {} items at {}", self.sender.capacity(), core::panic::Location::caller());
						match self.sender.broadcast(msg).await {
							Ok(None) => true,
							Ok(Some(_)) => unreachable!("async_broadcast feature unused"),
							Err(_) => false,
						}
					},
					async_broadcast::TrySendError::Closed(_msg) => false,
					async_broadcast::TrySendError::Inactive(_msg) =>
						unreachable!("async_broadcast feature unused"),
				},
			}
		}
	}
}
impl<T> Sender<T> {
	/// Creates a receiver, and reopens channel if it was previously closed
	pub fn receiver(&mut self) -> Receiver<T> {
		Receiver {
			receiver: {
				let receiver = self.sender.new_receiver();

				if receiver.is_closed() {
					let (new_sender, new_receiver) =
						async_broadcast::broadcast(self.sender.capacity());
					let _ = std::mem::replace(&mut self.sender, new_sender);
					new_receiver
				} else {
					receiver
				}
			},
			_detect_close_receiver: self.detect_close_sender.subscribe(), /* Will reopen channel
			                                                               * even if closed
			                                                               * previously */
		}
	}

	/// Waits until all receivers have been dropped, and therefore the channel is closed
	pub async fn closed(&self) {
		self.detect_close_sender.closed().await
	}
}

#[derive(Clone)]
pub struct Receiver<T> {
	receiver: async_broadcast::Receiver<T>,
	_detect_close_receiver: tokio::sync::watch::Receiver<()>,
}
impl<T: Clone> Stream for Receiver<T> {
	type Item = T;

	fn poll_next(
		mut self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Option<Self::Item>> {
		self.receiver.poll_next_unpin(cx)
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
		assert!(!sender.send(1).await);
		let mut receiver = sender.receiver();
		assert!(sender.send(1).await);
		assert!(sender.send(1).await);
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

		assert!(sender.send(1).await);

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

		assert!(sender.send(1).await);

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
