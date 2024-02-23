use std::time::Duration;

pub const DEFAULT_RETRY_DELAYS: &[Duration] = &[
	Duration::from_millis(100),
	Duration::from_millis(200),
	Duration::from_millis(400),
	Duration::from_millis(800),
	Duration::from_millis(1200),
	Duration::from_millis(2400),
	Duration::from_millis(4800),
	Duration::from_millis(9600),
];
