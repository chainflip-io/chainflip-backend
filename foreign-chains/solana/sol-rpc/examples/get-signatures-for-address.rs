use sol_rpc::{calls::GetSignaturesForAddress, traits::CallApi};

type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;

const HTTP_API_URL: &str = "https://api.devnet.solana.com:443/";

#[tokio::main]
async fn main() -> Result<(), AnyError> {
	let http_client = jsonrpsee::http_client::HttpClientBuilder::default().build(HTTP_API_URL)?;

	for address in std::env::args().skip(1) {
		let tx_list = http_client
			.call(&GetSignaturesForAddress::for_address(address.parse()?))
			.await?;

		eprintln!("{}:", address);
		for tx in tx_list {
			eprintln!("\t- {:?}", tx);
		}
	}
	Ok(())
}
