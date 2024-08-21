use crate::{SwapLimits, SwapLimitsProvider};

pub struct MockSwapLimitsProvider;

impl SwapLimitsProvider for MockSwapLimitsProvider {
	fn get_swap_limits() -> SwapLimits {
		SwapLimits {
			max_swap_retry_duration_blocks: 600_u32,
			max_swap_request_duration_blocks: 14400_u32,
		}
	}
}
