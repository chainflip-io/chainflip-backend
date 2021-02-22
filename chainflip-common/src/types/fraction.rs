use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, convert::TryInto, fmt, str};

/// Type alias for a Percentage fraction
/// Fraction of the total owned amount to unstake
pub type WithdrawFraction = PercentageFraction;

/// An atomic representation of a fraction with 2 significant digits
#[derive(Debug, Clone, Copy, Eq, PartialEq, Encode, Decode, Serialize, Deserialize)]
pub struct PercentageFraction(u32);

impl PercentageFraction {
    /// Value representing 100%
    pub const MAX: PercentageFraction = PercentageFraction(10_000);

    /// Create an instance if valid
    pub fn new(fraction: u32) -> Result<Self, &'static str> {
        if fraction < 1 || fraction > PercentageFraction::MAX.0 {
            Err("Fraction must be in the range (0; 10_000]")
        } else {
            Ok(PercentageFraction(fraction))
        }
    }

    /// Get the atomic fraction value
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Get the fraction value
    pub fn fraction(&self) -> f32 {
        self.0 as f32 / Self::MAX.0 as f32
    }
}

impl str::FromStr for PercentageFraction {
    type Err = &'static str;

    fn from_str(f: &str) -> Result<Self, Self::Err> {
        let fraction: f32 = f.parse().map_err(|_| "string must be a number")?;
        fraction.try_into()
    }
}

impl fmt::Display for PercentageFraction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<f32> for PercentageFraction {
    type Error = &'static str;

    /// Convert (0, 1] into a percentage fraction
    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value <= 0.0 || value > 1.0 {
            return Err("Value must be in range (0, 1]");
        }

        let atomic = (value * Self::MAX.0 as f32).trunc() as u32;
        PercentageFraction::new(atomic)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn correctly_creates_new_fraction() {
        assert_eq!(
            PercentageFraction::new(0).unwrap_err(),
            "Fraction must be in the range (0; 10_000]"
        );

        assert_eq!(
            PercentageFraction::new(10_001).unwrap_err(),
            "Fraction must be in the range (0; 10_000]"
        );

        assert!(PercentageFraction::new(1).is_ok());
    }

    #[test]
    fn correctly_parses_f32() {
        assert!(PercentageFraction::try_from(1.1f32).is_err());
        assert!(PercentageFraction::try_from(0f32).is_err());

        assert!(PercentageFraction::try_from(0.111f32).is_ok());
        assert!(PercentageFraction::try_from(1f32).is_ok());

        let fraction = PercentageFraction::try_from(0.3333987654f32);
        assert_eq!(fraction.unwrap().value(), 3333);
    }

    #[test]
    fn correctly_parses_string() {
        assert!(PercentageFraction::from_str("abc").is_err());
        assert!(PercentageFraction::from_str("1.1").is_err());
        assert!(PercentageFraction::from_str("0").is_err());

        assert!(PercentageFraction::from_str("0.1").is_ok());
        assert!(PercentageFraction::from_str("1.0").is_ok());

        let fraction = PercentageFraction::from_str("0.3333987654");
        assert_eq!(fraction.unwrap().value(), 3333);
    }

    #[test]
    fn correctly_returns_fraction() {
        assert_eq!(PercentageFraction::new(0001).unwrap().fraction(), 0.0001f32);
        assert_eq!(PercentageFraction::new(5134).unwrap().fraction(), 0.5134f32);
        assert_eq!(PercentageFraction::new(100_00).unwrap().fraction(), 1f32);
    }
}
