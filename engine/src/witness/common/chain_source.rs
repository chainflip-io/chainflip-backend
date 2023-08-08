pub mod and_then;
pub mod extension;
pub mod lag_safety;
pub mod logging;
pub mod shared;
pub mod strictly_monotonic;
pub mod then;

use std::pin::Pin;

use futures_core::{Future, Stream};

pub mod aliases {
	use codec::FullCodec;
	use num_traits::Bounded;
	use serde::{de::DeserializeOwned, Serialize};
	use std::iter::Step;

	macro_rules! define_trait_alias {
		(pub trait $name:ident: $($traits:tt)+) => {
			pub trait $name: $($traits)+ {}
			impl<T: $($traits)+> $name for T {}
		}
	}

	define_trait_alias!(pub trait Index: core::fmt::Debug + Bounded + DeserializeOwned + Serialize + FullCodec + Step + PartialEq + Eq + PartialOrd + Ord + Clone + Copy + Send + Sync + Unpin + 'static);
	define_trait_alias!(pub trait Hash: core::fmt::Debug + PartialEq + Eq + Clone + Copy + Send + Sync + Unpin + 'static);
	define_trait_alias!(pub trait Data: Send + Sync + Unpin + 'static);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header<Index, Hash, Data> {
	pub index: Index,
	pub hash: Hash,
	pub parent_hash: Option<Hash>,
	pub data: Data,
}
impl<Index: aliases::Index, Hash: aliases::Hash, Data: aliases::Data> Header<Index, Hash, Data> {
	pub fn map_data<T, F: FnOnce(Self) -> T>(self, f: F) -> Header<Index, Hash, T> {
		Header { index: self.index, hash: self.hash, parent_hash: self.parent_hash, data: f(self) }
	}

	pub async fn then_data<Fut: Future, F: FnOnce(Self) -> Fut>(
		self,
		f: F,
	) -> Header<Index, Hash, Fut::Output> {
		Header {
			index: self.index,
			hash: self.hash,
			parent_hash: self.parent_hash,
			data: f(self).await,
		}
	}
}
impl<Index, Hash, Data, Error> Header<Index, Hash, Result<Data, Error>>
where
	Index: aliases::Index,
	Hash: aliases::Hash,
	Data: aliases::Data,
	Error: aliases::Data,
{
	pub async fn and_then_data<
		T: aliases::Data,
		Fut: Future<Output = Result<T, Error>>,
		F: FnOnce(Header<Index, Hash, Data>) -> Fut,
	>(
		self,
		f: F,
	) -> Header<Index, Hash, Fut::Output> {
		Header {
			index: self.index,
			hash: self.hash,
			parent_hash: self.parent_hash,
			data: match self.data {
				Ok(data) =>
					f(Header {
						index: self.index,
						hash: self.hash,
						parent_hash: self.parent_hash,
						data,
					})
					.await,
				Err(error) => Err(error),
			},
		}
	}
}

#[async_trait::async_trait]
pub trait ChainSource: Send + Sync {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	type Client: ChainClient<Index = Self::Index, Hash = Self::Hash, Data = Self::Data>;

	async fn stream_and_client(
		&self,
	) -> (BoxChainStream<'_, Self::Index, Self::Hash, Self::Data>, Self::Client);
}

#[async_trait::async_trait]
pub trait ChainClient: Send + Sync + Clone {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data>;
}

pub trait ChainStream: Stream<Item = Header<Self::Index, Self::Hash, Self::Data>> + Send {
	type Index: aliases::Index;
	type Hash: aliases::Hash;
	type Data: aliases::Data;

	fn into_box<'a>(self) -> BoxChainStream<'a, Self::Index, Self::Hash, Self::Data>
	where
		Self: 'a + Sized,
	{
		Box::pin(self)
	}
}
impl<
		Index: aliases::Index,
		Hash: aliases::Hash,
		Data: aliases::Data,
		T: Stream<Item = Header<Index, Hash, Data>> + Send,
	> ChainStream for T
{
	type Index = Index;
	type Hash = Hash;
	type Data = Data;
}
pub type BoxChainStream<'a, Index, Hash, Data> = Pin<
	Box<
		dyn ChainStream<Index = Index, Hash = Hash, Data = Data, Item = Header<Index, Hash, Data>>
			+ Send
			+ 'a,
	>,
>;
