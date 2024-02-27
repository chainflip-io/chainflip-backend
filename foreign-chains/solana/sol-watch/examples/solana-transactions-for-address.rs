type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;

use std::{sync::atomic::AtomicBool, time::Duration};

use futures::TryStreamExt;
use sol_prim::{Address, SlotNumber};
use sol_watch::{
	address_transactions_stream::AddressSignatures, deduplicate_stream::DeduplicateStreamExt,
	ensure_balance_continuity::EnsureBalanceContinuityStreamExt,
	fetch_balance::FetchBalancesStreamExt,
};
use structopt::StructOpt;

#[derive(StructOpt)]
struct Args {
	#[structopt(long, short, env = "API_URL", default_value = "https://api.devnet.solana.com:443")]
	call_api: String,

	#[structopt(long, short, default_value = "1000")]
	page_size: usize,

	#[structopt(long, short, default_value = "0")]
	slot: SlotNumber,

	#[structopt(long, short, default_value = "1000")]
	dedup_backlog: usize,

	#[structopt(long, short)]
	terminate_after_secs: Option<u64>,

	#[structopt()]
	address: Address,
}

#[tokio::main]
async fn main() -> Result<(), AnyError> {
	let args: Args = StructOpt::from_args();

	let call_api = sol_rpc::retrying::Retrying::new(
		jsonrpsee::http_client::HttpClientBuilder::default().build(args.call_api.as_str())?,
		sol_rpc::retrying::Delays::default(),
	);

	let kill_switch = AtomicBool::default();

	let stream_running = AddressSignatures::new(&call_api, args.address, &kill_switch)
		.starting_with_slot(args.slot)
		.max_page_size(args.page_size)
		.into_stream()
		.deduplicate(
			args.dedup_backlog,
			|result| result.as_ref().ok().copied(),
			|tx_id, _| eprintln!("! {}", tx_id),
		)
		.fetch_balances(&call_api, args.address)
		.inspect_ok(|balance| {
			eprintln!(
					"discovered [{:^10}] {:^92}: Dep: {:^15}; Wit: {:^15}; [{:^5}]; Def: {:^15}; Pro: {:^15}",
					balance.slot,
					balance.signature.to_string(),
					balance.deposited().unwrap_or_default(),
					balance.withdrawn().unwrap_or_default(),
					if balance.discrepancy.is_reconciled() { "GO!" } else { "WAIT!" },
					balance.discrepancy.deficite,
					balance.discrepancy.proficite,
				)
		})
		.map_err(AnyError::from)
		.ensure_balance_continuity(args.page_size)
		.try_for_each(|balance| async move {
			Ok(eprintln!(
				"TO-WITNESS [{:^10}] {:^92}: DEP: {:^15}; WIT: {:^15}",
				balance.slot,
				balance.signature,
				balance.deposited().unwrap_or_default(),
				balance.withdrawn().unwrap_or_default()
			))
		});

	let kill_switch_running = async {
		if let Some(terminate_after) = args.terminate_after_secs {
			tokio::time::sleep(Duration::from_secs(terminate_after)).await;
			eprintln!("Terminating...");
			kill_switch.store(true, std::sync::atomic::Ordering::Relaxed);
		}
		Ok(())
	};

	futures::future::try_join(stream_running, kill_switch_running).await?;

	Ok(())
}
