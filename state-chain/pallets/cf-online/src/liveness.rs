use codec::{Decode, Encode};
use sp_runtime::RuntimeDebug;

pub type Liveness = u8;
pub const SUBMITTED: u8 = 1;

/// Liveness bitmap tracking intervals
pub trait LivenessTracker {
	/// Online status
	fn is_online(&self) -> bool;
	/// Update state of current interval
	fn update_current_interval(&mut self, online: bool) -> Self;
	/// State of submission for the current interval
	fn has_submitted(&self) -> bool;
}

impl LivenessTracker for Node {
	fn is_online(&self) -> bool {
		// Online for 2 * `HeartbeatBlockInterval` or 2 lsb
		self.liveness & 0x3 != 0
	}

	fn update_current_interval(&mut self, online: bool) -> Self {
		self.liveness <<= 1;
		self.liveness |= online as u8;
		*self
	}

	fn has_submitted(&self) -> bool {
		self.liveness & 0x1 == 0x1
	}
}

/// A representation of a node in the network
#[derive(Encode, Decode, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Node {
	pub liveness: Liveness,
	pub is_validator: bool,
}

/// The default node has submitted a heartbeat and is not a validator
impl Default for Node {
	fn default() -> Self {
		Node {
			liveness: SUBMITTED,
			is_validator: false,
		}
	}
}
