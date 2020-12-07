use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, convert::TryInto, fmt::Display, str::FromStr};

/// Type aliad for a Percentage fraction
pub type UnstakeFraction = PercentageFraction;

/// Fraction of the total owned amount to unstake
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
pub struct PercentageFraction(u32);

impl PercentageFraction {
    /// Value representing 100%
    pub const MAX: PercentageFraction = PercentageFraction(100_00);

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
        self.0 as f32 / 100f32
    }
}

impl FromStr for PercentageFraction {
    type Err = &'static str;

    fn from_str(f: &str) -> Result<Self, Self::Err> {
        let fraction: f32 = f.parse().map_err(|_| "fraction must be an integer")?;
        fraction.try_into()
    }
}

impl Display for PercentageFraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<f32> for PercentageFraction {
    type Error = &'static str;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        let atomic = (value * 100f32).trunc() as u32;
        PercentageFraction::new(atomic)
    }
}

#[cfg(test)]
mod test {
    use super::*;

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
        assert!(PercentageFraction::try_from(100.1f32).is_err());
        assert!(PercentageFraction::try_from(0f32).is_err());

        assert!(PercentageFraction::try_from(0.1f32).is_ok());
        assert!(PercentageFraction::try_from(100f32).is_ok());

        let fraction = PercentageFraction::try_from(33.33987654f32);
        assert_eq!(fraction.unwrap().value(), 3333);
    }

    #[test]
    fn correctly_parses_string() {
        assert!(PercentageFraction::from_str("abc").is_err());
        assert!(PercentageFraction::from_str("100.1").is_err());
        assert!(PercentageFraction::from_str("0").is_err());

        assert!(PercentageFraction::from_str("0.1").is_ok());
        assert!(PercentageFraction::from_str("100").is_ok());

        let fraction = PercentageFraction::from_str("33.33987654");
        assert_eq!(fraction.unwrap().value(), 3333);
    }

    #[test]
    fn correctly_returns_fraction() {
        assert_eq!(PercentageFraction::new(0001).unwrap().fraction(), 0.01f32);
        assert_eq!(PercentageFraction::new(5134).unwrap().fraction(), 51.34f32);
        assert_eq!(PercentageFraction::new(100_00).unwrap().fraction(), 100f32);
    }
}
