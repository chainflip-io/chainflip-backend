use parity_scale_codec::Encode;

use crate::relayer::EventSink;

pub struct StateChain {
    dummy: (),
}

#[async_trait]
impl<E> EventSink<E> for StateChain
where
    E: 'static + Send + Encode,
{
    async fn process_event(&self, event: E) {
        todo!()
    }
}
