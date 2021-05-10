use crate::{
    eth::EventSink,
    mq::mq::{self, IMQClient, Subject},
};

use async_trait::async_trait;

use super::stake_manager::StakingEvent;

pub struct StakeManagerSink<M: IMQClient + Send + Sync> {
    mq_client: M,
}

impl<M: IMQClient + Send + Sync> StakeManagerSink<M> {
    async fn new(&self, mq_options: mq::Options) -> Self {
        let mq_client = *M::connect(mq_options)
            .await
            .expect("StakeManagerSink cannot create message queue client");

        StakeManagerSink { mq_client }
    }
}

#[async_trait]
impl<M: IMQClient + Send + Sync> EventSink<StakingEvent> for StakeManagerSink<M> {
    async fn process_event(&self, event: StakingEvent) -> anyhow::Result<()> {
        self.mq_client.publish(Subject::Stake, &event).await?;
        Ok(())
    }
}
