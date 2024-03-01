//! Works fine until if there is no slot with multiple transactions :\

use std::{borrow::Borrow, collections::VecDeque, sync::atomic::AtomicBool, time::Duration};

use futures::{stream, Stream, TryStreamExt};
use sol_prim::{Address, Signature, SlotNumber};
use sol_rpc::{calls::GetSignaturesForAddress, traits::CallApi};

// NOTE: Solana default is 1000 but setting it explicitly
const DEFAULT_PAGE_SIZE_LIMIT: usize = 1000;
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

pub struct AddressSignatures<Api, K> {
	call_api: Api,
	address: Address,
	starting_with_slot: Option<SlotNumber>,
	ending_with_slot: Option<SlotNumber>,
	until_transaction: Option<Signature>,
	page_size_limit: usize,
	poll_interval: Duration,

	state: State,
	kill_switch: K,
}

impl<Api, K> AddressSignatures<Api, K> {
	pub fn new(call_api: Api, address: Address, kill_switch: K) -> Self {
		Self {
			call_api,
			address,
			starting_with_slot: None,
			ending_with_slot: None,
			until_transaction: None,
			page_size_limit: DEFAULT_PAGE_SIZE_LIMIT,
			poll_interval: DEFAULT_POLL_INTERVAL,

			state: State::GetHistory(Duration::ZERO, None),
			kill_switch,
		}
	}

	pub fn starting_with_slot(self, slot: SlotNumber) -> Self {
		Self { starting_with_slot: Some(slot), ..self }
	}
	pub fn ending_with_slot(self, slot: SlotNumber) -> Self {
		Self { ending_with_slot: Some(slot), ..self }
	}
	pub fn until_transaction(self, tx_id: Signature) -> Self {
		Self { until_transaction: Some(tx_id), ..self }
	}
	pub fn page_size_limit(self, page_size_limit: usize) -> Self {
		Self { page_size_limit, ..self }
	}
	pub fn poll_interval(self, poll_interval: Duration) -> Self {
		Self { poll_interval, ..self }
	}
}

impl<Api, K> AddressSignatures<Api, K>
where
	Api: CallApi,
	K: Borrow<AtomicBool>,
{
	pub fn into_stream(mut self) -> impl Stream<Item = Result<Signature, Api::Error>> {
		self.state = State::GetHistory(Duration::ZERO, self.until_transaction);
		stream::try_unfold(self, Self::unfold).try_filter_map(|opt| async move { Ok(opt) })
	}
}

enum State {
	GetHistory(Duration, Option<Signature>),
	Drain(VecDeque<Signature>, Option<Signature>),
}

impl<Api, K> AddressSignatures<Api, K>
where
	Api: CallApi,
	K: Borrow<AtomicBool>,
{
	async fn unfold(mut self) -> Result<Option<(Option<Signature>, Self)>, Api::Error> {
		if self.kill_switch.borrow().load(std::sync::atomic::Ordering::Relaxed) {
			return Ok(None)
		}

		let out = match self.state {
			State::GetHistory(sleep, last_signature) => {
				tokio::time::sleep(sleep).await;

				let mut history = VecDeque::new();
				get_transaction_history(
					&self.call_api,
					&mut history,
					self.address,
					self.starting_with_slot,
					self.ending_with_slot,
					last_signature,
					self.page_size_limit,
				)
				.await?;
				let last_signature = history.back().copied().or(last_signature);
				self.state = State::Drain(history, last_signature);

				Some((None, self))
			},
			State::Drain(mut queue, last_signature) =>
				if let Some(signature) = queue.pop_back() {
					self.state = State::Drain(queue, last_signature);
					Some((Some(signature), self))
				} else {
					self.state = State::GetHistory(self.poll_interval, last_signature);
					Some((None, self))
				},
		};
		Ok(out)
	}
}

async fn get_transaction_history<Api>(
	call_api: Api,
	output: &mut impl Extend<Signature>,

	address: Address,
	starting_with_slot: Option<SlotNumber>,
	ending_with_slot: Option<SlotNumber>,
	until_tx: Option<Signature>,

	page_size_limit: usize,
) -> Result<(), Api::Error>
where
	Api: CallApi,
{
	let mut before_tx = None;

	loop {
		let (page_size, reference_signature) = get_single_page(
			&call_api,
			output,
			address,
			starting_with_slot,
			ending_with_slot,
			until_tx,
			before_tx,
			page_size_limit,
		)
		.await?;

		// page_size should never be > max_page_size
		if page_size != page_size_limit {
			break Ok(())
		}

		before_tx = reference_signature;
	}
}

async fn get_single_page<Api>(
	call_api: Api,
	output: &mut impl Extend<Signature>,

	address: Address,
	starting_with_slot: Option<SlotNumber>,
	ending_with_slot: Option<SlotNumber>,
	until_tx: Option<Signature>,
	before_tx: Option<Signature>,

	page_size_limit: usize,
) -> Result<(usize, Option<Signature>), Api::Error>
where
	Api: CallApi,
{
	let request = GetSignaturesForAddress {
		before: before_tx,
		until: until_tx,
		limit: Some(page_size_limit),
		..GetSignaturesForAddress::for_address(address)
	};
	let page = call_api.call(request).await?;

	// TODO: Currently there is a bug with `getSignaturesForAddress` RPC method. Transactions in the
	// same slot might not be ordered correctly for time of execution. This might be a problem when
	// using "until" to stop the getSignaturesForAddress call.
	// https://github.com/solana-labs/solana/issues/22456
	// This is fixed in https://github.com/solana-labs/solana/pull/33419
	// The fix will be live on v1.18, which should be Q2. We need to make sure it's done or we could
	// either miss a transaction or double witness. Double witness is most likely not a problem
	// because the State Chain will reject it (right?). If so, the downside is not too bad in normal
	// scenarios as there shouldn't be two transactions in the same slot. Probably not worth the
	// workaround.

	// TODO: make sure the page is actually sorted by slot-number. We are now sorting it in the
	// rpc "process_response", we need to double check if it behaves well.

	let mut reference_signature = None;
	// Filtering out by slot number for when we reuse a channel, as we won't have the last
	// transaction signature. We can use ending_with_slot for when a channel closes (althoght it
	// is probably not be necessary)

	// the page is sorted newest-to-oldest
	let page_iter = page.into_iter();
	let page_len = page_iter.clone().count();

	let signatures = page_iter
		// skip those entries `e` for which `e.slot` is strictly higher than `ending_with_slot`
		// (if the latter is not specified — do not skip)
		.skip_while(|e| ending_with_slot.map(|s| e.slot > s).unwrap_or(false))
		// take those entries `e` for which `e.slot` is greater than or equal to
		// `starting_with_slot` (if the latter is not specified — take it anyway)
		.take_while(|e| starting_with_slot.map(|s| e.slot >= s).unwrap_or(true))
		.map(|e| {
			reference_signature = Some(e.signature);

			e.signature
		});

	output.extend(signatures);

	Ok((page_len, reference_signature))
}
