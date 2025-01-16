pub mod broker;
pub mod lp;

#[macro_export]
macro_rules! extract_event {
    ($events:expr, $runtime_event_variant:path, $pallet_event_variant:path, $pattern:tt, $result:expr) => {
        if let Some($runtime_event_variant($pallet_event_variant $pattern)) = $events.iter().find(|event| {
            matches!(event, $runtime_event_variant($pallet_event_variant { .. }))
        }) {
        	Ok($result)
        } else {
            Err(anyhow!("No {}({}) event was found", stringify!($runtime_event_variant), stringify!($pallet_event_variant)))
        }
    };
}
