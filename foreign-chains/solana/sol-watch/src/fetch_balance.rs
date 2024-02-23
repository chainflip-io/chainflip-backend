use std::{fmt, future::Future, pin::Pin, task::Poll};

use futures::{FutureExt, Stream, TryStream};
use sol_prim::{Address, Amount, Signature, SlotNumber};
use sol_rpc::{calls::GetTransaction, traits::CallApi};

#[derive(Debug, Clone, Copy)]
pub struct Balance {
	pub signature: Signature,
	pub slot: SlotNumber,
	pub before: Amount,
	pub after: Amount,

	pub discrepancy: Discrepancy,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Discrepancy {
	pub deficite: Amount,
	pub proficite: Amount,
}

pub trait FetchBalancesStreamExt: TryStream<Ok = Signature> + Sized {
	fn fetch_balances<'a, Rpc>(
		self,
		rpc: Rpc,
		address: Address,
	) -> FetchBalances<'a, Self, Rpc, Rpc::Error>
	where
		Rpc: CallApi + 'a,
		Self::Error: From<Rpc::Error>,
	{
		FetchBalances::new(self, rpc, address)
	}
}

type Busy<'a, C, E> = Pin<Box<dyn Future<Output = Result<(C, Option<Balance>), E>> + Send + 'a>>;

#[pin_project::pin_project]
pub struct FetchBalances<'a, S, C, E> {
	#[pin]
	inner: S,

	prev: Option<Balance>,
	#[pin]
	busy: Option<Busy<'a, C, E>>,

	rpc: Option<C>,

	address: Address,
}

impl Discrepancy {
	pub fn is_reconciled(&self) -> bool {
		self.deficite == self.proficite &&
			self.deficite != Amount::MAX &&
			self.proficite != Amount::MAX
	}
}

impl<'a, S, C, E> FetchBalances<'a, S, C, E> {
	pub fn new(inner: S, rpc: C, address: Address) -> Self {
		let rpc = Some(rpc);
		Self { inner, rpc, address, prev: None, busy: None }
	}
}

impl Balance {
	pub fn deposited(&self) -> Option<Amount> {
		self.after.checked_sub(self.before).filter(|a| *a != 0)
	}
	pub fn withdrawn(&self) -> Option<Amount> {
		self.before.checked_sub(self.after).filter(|a| *a != 0)
	}
}

impl<S> FetchBalancesStreamExt for S where S: TryStream<Ok = Signature> + Sized {}

impl<'a, S, C, E> Stream for FetchBalances<'a, S, C, E>
where
	S: TryStream<Ok = Signature>,
	C: CallApi<Error = E> + 'a,
	S::Error: From<E>,
{
	type Item = Result<Balance, S::Error>;

	fn poll_next(
		self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Option<Self::Item>> {
		let mut this = self.project();

		Poll::Ready(loop {
			if let Some(fut) = this.busy.as_mut().as_pin_mut() {
				let balance_result = std::task::ready!(fut.poll(cx));
				this.busy.set(None);

				let (rpc, balance_opt) = match balance_result {
					Err(reason) => break Some(Err(reason.into())),
					Ok(rpc_and_balance) => rpc_and_balance,
				};
				*this.rpc = Some(rpc);

				let Some(balance) = balance_opt else { continue };

				let balance = balance.account_for_previous(this.prev.as_ref());
				this.prev.replace(balance);

				break Some(Ok(balance))
			}

			let signature = match std::task::ready!(this.inner.as_mut().try_poll_next(cx)) {
				None => break None,
				Some(Err(reason)) => break Some(Err(reason)),
				Some(Ok(signature)) => signature,
			};

			let rpc = this
				.rpc
				.take()
				.expect("Invalid state: rpc hasn't been put back after being taken");
			this.busy.set(Some(get_balance(rpc, *this.address, signature).boxed()));
		})
	}
}

impl Balance {
	fn account_for_previous(mut self, prev: Option<&Balance>) -> Self {
		let prev_balance_after = prev.map(|b| b.after);

		let this_proficite = prev_balance_after
			.map(|prev_balance_after| self.before.saturating_sub(prev_balance_after))
			.unwrap_or_default();
		let this_deficite = prev_balance_after
			.map(|prev_balance_after| prev_balance_after.saturating_sub(self.before))
			.unwrap_or_default();

		let prev_discrepancy = prev.map(|b| b.discrepancy).unwrap_or_default();

		let prev_proficite = prev_discrepancy.proficite;
		let prev_deficite = prev_discrepancy.deficite;

		self.discrepancy = Discrepancy {
			proficite: prev_proficite.saturating_add(this_proficite),
			deficite: prev_deficite.saturating_add(this_deficite),
		};

		self
	}
}

async fn get_balance<Rpc>(
	rpc: Rpc,
	address: Address,
	signature: Signature,
) -> Result<(Rpc, Option<Balance>), Rpc::Error>
where
	Rpc: CallApi,
{
	let response = rpc.call(GetTransaction::for_signature(signature)).await?;
	let Some((before, after)) = response.balances(&address) else { return Ok((rpc, None)) };
	let balance =
		Balance { signature, slot: response.slot, before, after, discrepancy: Default::default() };
	Ok((rpc, Some(balance)))
}

impl fmt::Display for Discrepancy {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "proficite: {:^15}; deficite: {:^15}", self.proficite, self.deficite)
	}
}
