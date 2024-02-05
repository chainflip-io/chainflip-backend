use core::fmt;

use super::Commitment;

const PROCESSED: &str = "processed";
const CONFIRMED: &str = "confirmed";
const FINALIZED: &str = "finalized";

impl std::str::FromStr for Commitment {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			PROCESSED => Ok(Self::Processed),
			CONFIRMED => Ok(Self::Confirmed),
			FINALIZED => Ok(Self::Finalized),
			invalid => Err(format!(
				"Invalid value: {}. Expected {}|{}|{}",
				invalid, PROCESSED, CONFIRMED, FINALIZED
			)),
		}
	}
}

impl fmt::Display for Commitment {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Confirmed => CONFIRMED,
			Self::Processed => PROCESSED,
			Self::Finalized => FINALIZED,
		}
		.fmt(f)
	}
}
