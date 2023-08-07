use super::{
	chunked_by_time::{builder::ChunkedByTimeBuilder, ChunkedByTime},
	chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn logging(self, log_prefix: &'static str) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault,
	{
		self.then(move |epoch, header| async move {
			tracing::info!(
				"{} | {} processed: epoch index: {:?}, block index {:?}, hash {:?}",
				<Inner::Chain as cf_chains::Chain>::NAME,
				log_prefix,
				epoch.index,
				header.index,
				header.hash
			);
			Ok::<_, anyhow::Error>(header.data)
		})
	}
}

impl<Inner: ChunkedByTime> ChunkedByTimeBuilder<Inner> {
	pub fn logging(self, log_prefix: &'static str) -> ChunkedByTimeBuilder<impl ChunkedByTime>
	where
		Inner: ChunkedByTime,
	{
		self.then(move |epoch, header| async move {
			tracing::info!(
				"{} | {} processed: epoch index: {:?}, block index {:?}, hash {:?}",
				<Inner::Chain as cf_chains::Chain>::NAME,
				log_prefix,
				epoch.index,
				header.index,
				header.hash
			);
			Ok::<_, anyhow::Error>(header.data)
		})
	}
}
