use crate::relayer::EventSink;

pub struct Stdout;

#[async_trait]
impl<E> EventSink<E> for Stdout
where
    E: 'static + Send + std::fmt::Debug,
{
    async fn process_event(&self, event: E) {
        log::debug!("Received event: {:?}", event);
    }
}
