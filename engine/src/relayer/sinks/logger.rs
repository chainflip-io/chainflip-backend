use crate::relayer::EventSink;

pub struct Logger;

#[async_trait]
impl<E> EventSink<E> for Logger
where
    E: 'static + Send + std::fmt::Debug,
{
    async fn process_event(&self, event: E) {
        log::info!("Received event: {:?}", event);
    }
}
