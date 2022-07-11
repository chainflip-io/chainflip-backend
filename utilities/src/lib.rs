#![cfg_attr(not(feature = "std"), no_std)]

/// Note that the resulting `threshold` is the maximum number
/// of parties *not* enough to generate a signature,
/// i.e. at least `t+1` parties are required.
/// This follows the notation in the multisig library that
/// we are using and in the corresponding literature.
///
/// For the *success* threshold, use [success_threshold_from_share_count].
pub fn threshold_from_share_count(share_count: u32) -> u32 {
    if 0 == share_count {
        0
    } else {
        (share_count.checked_mul(2).unwrap() - 1) / 3
    }
}

/// Returns the number of parties required for a threshold signature
/// ceremony to *succeed*.
pub fn success_threshold_from_share_count(share_count: u32) -> u32 {
    threshold_from_share_count(share_count)
        .checked_add(1)
        .unwrap()
}

/// Returns the number of bad parties required for a threshold signature
/// ceremony to *fail*.
pub fn failure_threshold_from_share_count(share_count: u32) -> u32 {
    share_count - threshold_from_share_count(share_count)
}

#[test]
fn check_threshold_calculation() {
    assert_eq!(threshold_from_share_count(150), 99);
    assert_eq!(threshold_from_share_count(100), 66);
    assert_eq!(threshold_from_share_count(90), 59);
    assert_eq!(threshold_from_share_count(3), 1);
    assert_eq!(threshold_from_share_count(4), 2);

    assert_eq!(success_threshold_from_share_count(150), 100);
    assert_eq!(success_threshold_from_share_count(100), 67);
    assert_eq!(success_threshold_from_share_count(90), 60);
    assert_eq!(success_threshold_from_share_count(3), 2);
    assert_eq!(success_threshold_from_share_count(4), 3);

    assert_eq!(failure_threshold_from_share_count(150), 51);
    assert_eq!(failure_threshold_from_share_count(100), 34);
    assert_eq!(failure_threshold_from_share_count(90), 31);
    assert_eq!(failure_threshold_from_share_count(3), 2);
    assert_eq!(failure_threshold_from_share_count(4), 2);
}

pub fn clean_eth_address(dirty_eth_address: &str) -> Result<[u8; 20], &str> {
    let eth_address_hex_str = match dirty_eth_address.strip_prefix("0x") {
        Some(eth_address_stripped) => eth_address_stripped,
        None => dirty_eth_address,
    };

    let eth_address: [u8; 20] = hex::decode(eth_address_hex_str)
        .map_err(|_| "Invalid hex")?
        .try_into()
        .map_err(|_| "Could not create a [u8; 20]")?;

    Ok(eth_address)
}

#[test]
fn cleans_eth_address() {
    // fail too short
    let input = "0x323232";
    assert!(clean_eth_address(input).is_err());

    // fail invalid chars
    let input = "0xZ29aB9EbDb421CE48b70flippya6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_err());

    // success with 0x
    let input = "0xB29aB9EbDb421CE48b70699758a6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_ok());

    // success without 0x
    let input = "B29aB9EbDb421CE48b70699758a6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_ok());
}

#[cfg(feature = "std")]
mod with_std {
    use core::fmt::Display;

    // Needed due to the jsonrpc maintainer's not definitely unquestionable decision to impl their error types without the Sync trait
    pub fn rpc_error_into_anyhow_error(error: jsonrpc_core_client::RpcError) -> anyhow::Error {
        anyhow::Error::msg(error.to_string())
    }

    pub trait JsonResultExt {
        type T;

        fn map_to_json_error(self) -> jsonrpc_core::Result<Self::T>;
    }

    impl<T, E: Display> JsonResultExt for std::result::Result<T, E> {
        type T = T;

        fn map_to_json_error(self) -> jsonrpc_core::Result<Self::T> {
            self.map_err(|error| jsonrpc_core::Error {
                code: jsonrpc_core::ErrorCode::ServerError(1),
                message: error.to_string(),
                data: None,
            })
        }
    }

    pub mod mockall_utilities {
        use mockall::Predicate;
        use predicates::reflection::PredicateReflection;

        // Allows equality predicate between differing types
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct EqPredicate<T> {
            constant: T,
        }
        impl<T: std::fmt::Debug, P: ?Sized> Predicate<P> for EqPredicate<T>
        where
            P: std::fmt::Debug + PartialEq<T>,
        {
            fn eval(&self, variable: &P) -> bool {
                variable.eq(&self.constant)
            }
        }
        impl<T: std::fmt::Debug> PredicateReflection for EqPredicate<T> {}
        impl<T: std::fmt::Debug> std::fmt::Display for EqPredicate<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "var {:?}", self.constant)
            }
        }
        pub fn eq<T>(constant: T) -> EqPredicate<T> {
            EqPredicate { constant }
        }
    }
}

#[cfg(feature = "std")]
pub use with_std::*;
