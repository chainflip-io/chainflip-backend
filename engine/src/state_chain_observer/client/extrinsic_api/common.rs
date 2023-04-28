use tokio::sync::{mpsc, oneshot};

pub(super) async fn send_request<Request, F: FnOnce(oneshot::Sender<Result>) -> Request, Result>(
	request_sender: &mpsc::Sender<Request>,
	into_request: F,
) -> oneshot::Receiver<Result> {
	let (result_sender, result_receiver) = oneshot::channel();
	// Must drop this _result before await'ing on result_receiver, as in error case it contains the
	// result_sender
	let _result = request_sender.send(into_request(result_sender)).await;
	result_receiver
}
