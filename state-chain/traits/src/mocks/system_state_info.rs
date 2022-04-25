use sp_runtime::DispatchError;

use crate::SystemStateInfo;

pub struct MockSystemStateInfo;

impl SystemStateInfo for MockSystemStateInfo {
	fn ensure_no_maintanace() -> Result<(), DispatchError> {
		Ok(())
	}
}
