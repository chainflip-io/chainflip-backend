use futures::{stream, Stream, StreamExt};

use crate::witness::common::{
	epoch_source::{Epoch, EpochSource},
	ActiveAndFuture,
};

pub async fn epoch_stream<I, H>(epoch_source: EpochSource<I, H>) -> impl Stream<Item = Epoch<I, H>>
where
	I: Clone + Send + Sync + 'static,
	H: Clone + Send + Sync + 'static,
{
	let ActiveAndFuture { active, future } = epoch_source.into_stream().await;
	stream::iter(active).chain(future)
}
