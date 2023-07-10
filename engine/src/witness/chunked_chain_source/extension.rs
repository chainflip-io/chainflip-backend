use futures_core::Future;

use crate::witness::{
	chain_source::{aliases, Header},
	epoch_source::Epoch,
};

use super::{map::Map, Builder, ChunkedChainSource};

impl<T: ChunkedChainSource> Builder<T> {
	pub fn map<MappedTo, FutMappedTo, MapFn>(self, map_fn: MapFn) -> Builder<Map<T, MapFn>>
	where
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send,
		MapFn: Fn(Epoch<T::Info, T::HistoricInfo>, Header<T::Index, T::Hash, T::Data>) -> FutMappedTo
			+ Send
			+ Sync
			+ Clone,
	{
		Builder { source: Map::new(self.source, map_fn), parameters: self.parameters }
	}
}
