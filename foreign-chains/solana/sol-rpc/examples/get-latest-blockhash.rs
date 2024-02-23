use sol_rpc::{calls::GetLatestBlockhash, traits::CallApi};

type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;

const HTTP_API_URL: &str = "https://api.devnet.solana.com:443/";

#[tokio::main]
async fn main() -> Result<(), AnyError> {
	let http_client = jsonrpsee::http_client::HttpClientBuilder::default().build(HTTP_API_URL)?;
	let response = http_client.call(&GetLatestBlockhash::default()).await?;
	eprintln!("latest-blockhash: {:?}", response);
	Ok(())
}
