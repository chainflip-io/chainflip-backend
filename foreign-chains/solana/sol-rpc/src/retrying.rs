use std::time::Duration;

use crate::traits::{Call, CallApi};

pub trait RetryPolicy {
	type Delays: Iterator<Item = Duration> + Send;
	fn delays(&self) -> Self::Delays;
}

#[derive(Debug, Clone, Copy)]
pub struct Delays<'a>(pub &'a [Duration]);

#[derive(Debug, Clone)]
pub struct Retrying<U, P> {
	underlying: U,
	policy: P,
}

#[derive(Debug, thiserror::Error)]
pub struct Error<E>(pub Vec<E>);

impl<U, P> Retrying<U, P> {
	pub fn new(underlying: U, policy: P) -> Self {
		Self { underlying, policy }
	}
}

#[async_trait::async_trait]
impl<B, P> CallApi for Retrying<B, P>
where
	B: CallApi + Send + Sync,
	P: RetryPolicy + Send + Sync,
{
	type Error = Error<B::Error>;
	async fn call<C: Call>(&self, call: C) -> Result<C::Response, Self::Error> {
		let mut errors = vec![];

		let delays = [Duration::ZERO].into_iter().chain(self.policy.delays());

		for d in delays {
			tokio::time::sleep(d).await;
			match self.underlying.call(&call).await {
				Ok(response) => return Ok(response),
				Err(reason) => {
					errors.push(reason);
				},
			}
		}

		Err(Error(errors))
	}
}

impl<'a> Default for Delays<'a> {
	fn default() -> Self {
		Self(crate::consts::DEFAULT_RETRY_DELAYS)
	}
}

impl<'a> RetryPolicy for Delays<'a> {
	type Delays = std::iter::Copied<std::slice::Iter<'a, Duration>>;
	fn delays(&self) -> Self::Delays {
		self.0.iter().copied()
	}
}

impl<E> std::fmt::Display for Error<E>
where
	E: std::fmt::Display,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		writeln!(f, "Out of retries. Underlying errors:")?;
		for inner in &self.0 {
			writeln!(f, "- {}", inner)?;
		}
		Ok(())
	}
}
