use crate::Port;
use serde::Deserialize;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::{fmt::format::FmtSpan, util::SubscriberInitExt};
use warp::{Filter, Reply};

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct LoggingSettings {
	pub span_lifecycle: bool,
	pub command_server_port: Port,
}

#[derive(Debug)]
pub enum ErrorType {
	Error(anyhow::Error),
	Panic,
}

#[macro_export]
macro_rules! print_start_and_end {
	(async $e:expr) => {
		$crate::print_start_and_end!(@ ::std::panic::AssertUnwindSafe($e).catch_unwind().await);
	};
	($e:expr) => {
		$crate::print_start_and_end!(@ ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| $e)));
	};
	(@ $e:expr) => {
		{
			println!(
				"Starting {} v{} ({})",
				env!("CARGO_PKG_NAME"),
				env!("CARGO_PKG_VERSION"),
				$crate::internal_lazy_format!(if let Some(repository_link) = $crate::repository_link() => ("CI Build: \"{}\"", repository_link) else => ("Non-CI Build"))
			);
			println!(
				"
				 ██████╗██╗  ██╗ █████╗ ██╗███╗   ██╗███████╗██╗     ██╗██████╗
				██╔════╝██║  ██║██╔══██╗██║████╗  ██║██╔════╝██║     ██║██╔══██╗
				██║     ███████║███████║██║██╔██╗ ██║█████╗  ██║     ██║██████╔╝
				██║     ██╔══██║██╔══██║██║██║╚██╗██║██╔══╝  ██║     ██║██╔═══╝
				╚██████╗██║  ██║██║  ██║██║██║ ╚████║██║     ███████╗██║██║
				 ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝╚═╝  ╚═══╝╚═╝     ╚══════╝╚═╝╚═╝
				"
			);

			match $e {
				Ok(result) => match result {
					Ok(_) => {
						Ok(())
					},
					Err(error) => {
						println!("Exiting {} due to error: {error:?}", env!("CARGO_PKG_NAME"));
						Err(utilities::logging::ErrorType::Error(error))
					},
				},
				Err(panic) => {
					// We'll never catch a panic since we use sp-panic-handler which set up a panic hook and abort the process
					println!(
						"Exiting {} due to panic: {:#?}",
						env!("CARGO_PKG_NAME"),
						panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned())
					);
					Err(utilities::logging::ErrorType::Panic)
				},
			}
		}
	};
}

/// Install a tracing subscriber that uses json formatting for the logs. The initial filtering
/// directives can be set using the RUST_LOG environment variable, if it is not set the subscriber
/// will default to INFO, meaning all INFO, WARN, or ERROR logs will be output, all the other logs
/// will be ignored. The filtering directives can also be controlled via a REST api while the
/// application is running, for example:
///
/// `curl -X GET 127.0.0.1:36079/tracing` - This returns the current filtering directives
/// `curl --json '"debug,warp=off,hyper=off,jsonrpc=off,web3=off,reqwest=off"'
/// 127.0.0.1:36079/tracing` - This sets the filter directives so the default is DEBUG, and the
/// logging in modules warp, hyper, jsonrpc, web3, and reqwest is turned off.
///
/// The above --json command is short hand for: `curl -X POST -H 'Content-Type: application/json' -d
/// '"debug,warp=off,hyper=off,jsonrpc=off,web3=off,reqwest=off"' 127.0.0.1:36079/tracing
///
/// The full syntax used for specifying filter directives used in both the REST api and in the RUST_LOG environment variable is specified here: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html
pub async fn init_json_logger(settings: LoggingSettings) -> DefaultGuard {
	use tracing::metadata::LevelFilter;
	use tracing_subscriber::EnvFilter;

	let format_span = if settings.span_lifecycle { FmtSpan::FULL } else { FmtSpan::NONE };

	let (reload_handle, _guard) = {
		let builder = tracing_subscriber::fmt()
			.json()
			.with_current_span(false)
			.with_span_list(true)
			.with_env_filter(
				EnvFilter::builder()
					.with_default_directive(LevelFilter::INFO.into())
					.from_env_lossy(),
			)
			.with_span_events(format_span)
			.with_filter_reloading();

		let reload_handle = builder.reload_handle();
		let _guard = builder.finish().set_default();
		(reload_handle, _guard)
	};

	tokio::task::spawn(async move {
		const PATH: &str = "tracing";
		const MAX_CONTENT_LENGTH: u64 = 2 * 1024;

		let change_filter = warp::post()
			.and(warp::path(PATH))
			.and(warp::path::end())
			.and(warp::body::content_length_limit(MAX_CONTENT_LENGTH))
			.and(warp::body::json())
			.then({
				let reload_handle = reload_handle.clone();
				move |filter: String| {
					futures::future::ready(
						match EnvFilter::builder()
							.with_default_directive(LevelFilter::INFO.into())
							.parse(filter)
						{
							Ok(env_filter) => match reload_handle.reload(env_filter) {
								Ok(_) => warp::reply().into_response(),
								Err(error) => warp::reply::with_status(
									warp::reply::json(&error.to_string()),
									warp::http::StatusCode::INTERNAL_SERVER_ERROR,
								)
								.into_response(),
							},
							Err(error) => warp::reply::with_status(
								warp::reply::json(&error.to_string()),
								warp::http::StatusCode::BAD_REQUEST,
							)
							.into_response(),
						},
					)
				}
			});

		let get_filter = warp::get().and(warp::path(PATH)).and(warp::path::end()).then(move || {
			futures::future::ready({
				let (status, message) =
					match reload_handle.with_current(|env_filter| env_filter.to_string()) {
						Ok(reply) => (warp::http::StatusCode::OK, reply),
						Err(error) =>
							(warp::http::StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
					};

				warp::reply::with_status(warp::reply::json(&message), status).into_response()
			})
		});

		warp::serve(change_filter.or(get_filter))
			.run((std::net::Ipv4Addr::LOCALHOST, settings.command_server_port))
			.await;
	});

	_guard
}
