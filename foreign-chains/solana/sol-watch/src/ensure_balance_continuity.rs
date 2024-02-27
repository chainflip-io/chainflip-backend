use std::{
	collections::VecDeque,
	pin::Pin,
	task::{Context, Poll},
};

use futures::{Stream, TryStream};
use sol_prim::SlotNumber;

use crate::fetch_balance::{Balance, Discrepancy};

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Inconsistent balance at the end of the slot #{}: {}", _0, _1)]
	InconsistentSlotBalances(SlotNumber, Discrepancy),

	#[error("Buffer exhausted [limit: {}]", _0)]
	BufferExhausted(usize),
}

pub trait EnsureBalanceContinuityStreamExt: Sized {
	fn ensure_balance_continuity(self, max_buffer_size: usize) -> EnsureBalanceContinuity<Self> {
		EnsureBalanceContinuity::new(self, max_buffer_size)
	}
}

#[derive(Debug, Clone)]
#[pin_project::pin_project]
pub struct EnsureBalanceContinuity<S> {
	#[pin]
	inner: S,
	max_buffer_size: usize,
	buffer: VecDeque<Balance>,
}

impl<S> EnsureBalanceContinuity<S> {
	pub fn new(inner: S, max_buffer_size: usize) -> Self {
		Self { inner, max_buffer_size, buffer: VecDeque::with_capacity(max_buffer_size + 1) }
	}
}

impl<S> EnsureBalanceContinuityStreamExt for S
where
	S: TryStream<Ok = Balance>,
	S::Error: From<Error>,
{
}

impl<S> Stream for EnsureBalanceContinuity<S>
where
	S: TryStream<Ok = Balance>,
	S::Error: From<Error>,
{
	type Item = Result<S::Ok, S::Error>;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let mut this = self.project();
		Poll::Ready(loop {
			match this.buffer.back() {
				Some(last_buffered) if last_buffered.discrepancy.is_reconciled() => {
					let balance = this
						.buffer
						.pop_front()
						.expect("we've just ensured the buffer is not empty");
					break Some(Ok(balance))
				},
				_ => {
					let next_opt = std::task::ready!(this.inner.as_mut().try_poll_next(cx));

					let Some(next_result) = next_opt else {
						if let Some(problem) = this.buffer.back() {
							break Some(Err(Error::InconsistentSlotBalances(
								problem.slot,
								problem.discrepancy,
							)
							.into()))
						} else {
							break None
						}
					};

					let next = match next_result {
						Err(reason) => break Some(Err(reason)),
						Ok(balance) => balance,
					};

					this.buffer.push_back(next);

					if this.buffer.len() > *this.max_buffer_size {
						break Some(Err(Error::BufferExhausted(*this.max_buffer_size).into()))
					}
				},
			}
		})
	}
}
