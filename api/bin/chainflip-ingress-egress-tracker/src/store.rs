use async_trait::async_trait;
use redis::{aio::MultiplexedConnection, AsyncCommands};
use serde::Serialize;
use std::time::Duration;

#[async_trait]
pub trait Store: Sync + Send + 'static {
	type Output: Sync + Send + 'static;

	async fn save_to_array<S: Storable>(&mut self, storable: &S) -> anyhow::Result<Self::Output>;
	async fn save_singleton<S: Storable>(&mut self, storable: &S) -> anyhow::Result<Self::Output>;
}

#[derive(Clone)]
pub struct RedisStore {
	con: MultiplexedConnection,
}

impl RedisStore {
	const EXPIRY_TIME: Duration = Duration::from_secs(3600);

	pub fn new(con: MultiplexedConnection) -> Self {
		Self { con }
	}
}

#[async_trait]
impl Store for RedisStore {
	type Output = ();

	async fn save_to_array<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()> {
		let key = storable.get_key();
		self.con.rpush(&key, serde_json::to_string(storable)?).await?;
		self.con.expire(key, Self::EXPIRY_TIME.as_secs() as i64).await?;

		Ok(())
	}

	async fn save_singleton<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()> {
		self.con
			.set_ex(
				storable.get_key(),
				serde_json::to_string(storable)?,
				Self::EXPIRY_TIME.as_secs(),
			)
			.await?;

		Ok(())
	}
}

#[async_trait]
pub trait Storable: Serialize + Sized + Sync + 'static {
	fn get_key(&self) -> String;
}
