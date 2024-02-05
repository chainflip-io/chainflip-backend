pub type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Transport: {}", _0)]
	Transport(#[source] AnyError),
}

impl Error {
	pub fn transport(e: impl Into<AnyError>) -> Self {
		Self::Transport(e.into())
	}
}
