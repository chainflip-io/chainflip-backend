pub type Liveness = u8;
pub const SUBMITTED: u8 = 1;

/// Liveness bitmap tracking intervals
pub trait LivenessTracker {
	/// Online status
	fn is_online(self) -> bool;
	/// Update state of current interval
	fn update_current_interval(&mut self, online: bool) -> Self;
	/// State of submission for the current interval
	fn has_submitted(self) -> bool;
}

impl LivenessTracker for Liveness {
	fn is_online(self) -> bool {
		// Online for 2 * `HeartbeatBlockInterval` or 2 lsb
		self & 0x3 != 0
	}

	fn update_current_interval(&mut self, online: bool) -> Self {
		*self <<= 1;
		*self |= online as u8;
		*self
	}

	fn has_submitted(self) -> bool {
		self & 0x1 == 0x1
	}
}
