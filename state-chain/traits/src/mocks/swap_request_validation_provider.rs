use crate::{SwapRequestValidation, SwapRequestValidationProvider};

pub struct MockSwapRequestValidationProvider;

impl SwapRequestValidationProvider for MockSwapRequestValidationProvider {
	fn get_swap_request_limits() -> SwapRequestValidation {
		SwapRequestValidation {
			max_swap_retry_duration_blocks: 600_u32,
			max_swap_request_duration_blocks: 14400_u32,
		}
	}
}
