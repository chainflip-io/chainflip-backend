use futures_core::Future;

use crate::witness::{
	chain_source::{aliases, Header},
	epoch_source::Epoch,
};

use super::{then::Then, Builder, ChunkedChainSource};

impl<T: ChunkedChainSource> Builder<T> {
	pub fn then<Output, Fut, ThenFn>(self, then_fn: ThenFn) -> Builder<Then<T, ThenFn>>
	where
		Output: aliases::Data,
		Fut: Future<Output = Output> + Send,
		ThenFn: Fn(Epoch<T::Info, T::HistoricInfo>, Header<T::Index, T::Hash, T::Data>) -> Fut
			+ Send
			+ Sync
			+ Clone,
	{
		Builder { source: Then::new(self.source, then_fn), parameters: self.parameters }
	}
}
