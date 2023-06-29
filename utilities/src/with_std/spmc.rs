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
