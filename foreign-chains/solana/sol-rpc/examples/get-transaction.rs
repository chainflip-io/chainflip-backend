use sol_rpc::{
	calls::GetTransaction,
	retrying::{Delays, Retrying},
	traits::CallApi,
};

type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;

const HTTP_API_URL: &str = "https://api.devnet.solana.com:443/";

#[tokio::main]
async fn main() -> Result<(), AnyError> {
	let http_client = jsonrpsee::http_client::HttpClientBuilder::default().build(HTTP_API_URL)?;
	let http_client = Retrying::new(http_client, Delays::default());

	for tx_sig in std::env::args().skip(1) {
		let tx_info = http_client.call(&GetTransaction::for_signature(tx_sig.parse()?)).await?;
		// eprintln!("{}: {:#?}", tx_sig, tx_info);
		eprintln!("{}:", tx_sig);
		for acc in tx_info.addresses() {
			let Some((pre, post)) = tx_info.balances(acc) else { continue };
			eprintln!("\t{}: {} -> {}", acc, pre, post);
		}
	}
	Ok(())
}
