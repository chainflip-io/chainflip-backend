use async_trait::async_trait;
use redis::{aio::MultiplexedConnection, AsyncCommands};
use serde::Serialize;
use std::time::Duration;

#[async_trait]
pub trait Store: Sync + Send + 'static {
	async fn save_to_array<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()>;
	async fn save_singleton<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct RedisStore {
	con: MultiplexedConnection,
}

impl RedisStore {
	pub fn new(con: MultiplexedConnection) -> Self {
		Self { con }
	}
}

#[async_trait]
impl Store for RedisStore {
	async fn save_to_array<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()> {
		let key = storable.key();
		self.con
			.rpush::<String, String, ()>(key.clone(), serde_json::to_string(storable)?)
			.await?;
		self.con
			.expire::<String, ()>(key, storable.expiry_duration().as_secs() as i64)
			.await?;

		Ok(())
	}

	async fn save_singleton<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()> {
		self.con
			.set_ex::<String, String, ()>(
				storable.key(),
				serde_json::to_string(storable)?,
				storable.expiry_duration().as_secs(),
			)
			.await?;

		Ok(())
	}
}

pub trait Storable: Serialize + Sized + Sync + 'static {
	const DEFAULT_EXPIRY_DURATION: Duration = Duration::from_secs(3600);

	fn key(&self) -> String;

	fn expiry_duration(&self) -> Duration {
		Self::DEFAULT_EXPIRY_DURATION
	}
}
