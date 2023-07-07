use futures_core::Future;
use utilities::task_scope::Scope;

use super::{
	aliases, lag_safety::LagSafety, map::Map, shared::SharedSource,
	strictly_monotonic::StrictlyMonotonic, ChainSource, Header,
};

pub trait ChainSourceExt: ChainSource {
	fn map<MappedTo, FutMappedTo, MapFn>(self, map_fn: MapFn) -> Map<Self, MapFn>
	where
		Self: Sized,
		MappedTo: aliases::Data,
		FutMappedTo: Future<Output = MappedTo> + Send + Sync,
		MapFn: Fn(Header<Self::Index, Self::Hash, Self::Data>) -> FutMappedTo + Send + Sync + Clone,
	{
		Map::new(self, map_fn)
	}

	fn lag_safety(self, margin: usize) -> LagSafety<Self>
	where
		Self: Sized,
	{
		LagSafety::new(self, margin)
	}

	fn shared<'env>(self, scope: &Scope<'env, anyhow::Error>) -> SharedSource<Self>
	where
		Self: 'env + Sized,
		Self::Client: Clone,
		Self::Data: Clone,
	{
		SharedSource::new(self, scope)
	}

	fn strictly_monotonic(self) -> StrictlyMonotonic<Self>
	where
		Self: Sized,
	{
		StrictlyMonotonic::new(self)
	}
}
