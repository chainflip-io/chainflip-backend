// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use super::{super::ECPoint, Scalar};

type PK = curve25519_dalek::edwards::EdwardsPoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Point(PK);

// Manual `Deserialize` (PRO-2856): `curve25519-dalek`'s default impl for
// `EdwardsPoint` only validates that the encoded point lies on the curve;
// it does NOT enforce prime-order subgroup membership. FROST and Pedersen
// DKG security proofs assume a prime-order group, so we reject points that
// contain a torsion component before they enter the protocol.
impl<'de> Deserialize<'de> for Point {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let inner = PK::deserialize(deserializer)?;
		if !inner.is_torsion_free() {
			return Err(serde::de::Error::custom(
				"ed25519 point is not in the prime-order subgroup",
			));
		}
		Ok(Point(inner))
	}
}
mod point_impls {

	use super::*;

	impl Ord for Point {
		fn cmp(&self, other: &Self) -> std::cmp::Ordering {
			self.as_bytes().cmp(&other.as_bytes())
		}
	}

	impl PartialOrd for Point {
		fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
			Some(self.cmp(other))
		}
	}

	impl ECPoint for Point {
		type Scalar = Scalar;

		type CompressedPointLength = typenum::U32;

		fn from_scalar(scalar: &Self::Scalar) -> Self {
			Point(curve25519_dalek::constants::ED25519_BASEPOINT_POINT * scalar.0)
		}

		fn as_bytes(&self) -> generic_array::GenericArray<u8, Self::CompressedPointLength> {
			self.0.compress().to_bytes().into()
		}

		fn point_at_infinity() -> Self {
			use curve25519_dalek::traits::Identity;
			Point(PK::identity())
		}
	}

	derive_point_impls!(Point, Scalar);

	impl std::ops::Add for Point {
		type Output = Self;

		fn add(self, rhs: Self) -> Self::Output {
			Point(self.0 + rhs.0)
		}
	}

	impl std::ops::Sub for Point {
		type Output = Self;

		fn sub(self, rhs: Self) -> Self::Output {
			Point(self.0 - rhs.0)
		}
	}

	impl<B: std::borrow::Borrow<Scalar>> std::ops::Mul<B> for Point {
		type Output = Self;

		fn mul(self, rhs: B) -> Self::Output {
			Point(self.0 * rhs.borrow().0)
		}
	}
}

#[test]
fn sanity_check_point_at_infinity() {
	use super::ECScalar;
	// Sanity check: point at infinity should correspond
	// to "zero" on the elliptic curve
	assert_eq!(Point::point_at_infinity(), Point::from_scalar(&Scalar::zero()));
}

/// This test asserts that deserialising a torsion-shifted point must error.
#[test]
fn deserialization_rejects_torsion_shifted_point() {
	use curve25519_dalek::{constants, edwards::EdwardsPoint, traits::Identity};

	// `EIGHT_TORSION[0]` is the identity; the others are non-trivial
	// small-order elements (orders 2, 4, or 8).
	let torsion: EdwardsPoint = constants::EIGHT_TORSION[1];
	assert_ne!(torsion, EdwardsPoint::identity());
	assert!(!torsion.is_torsion_free());

	// A valid prime-order point shifted by a torsion element. This is what a
	// malicious DKG / FROST participant could submit as a coefficient commitment.
	let shifted = constants::ED25519_BASEPOINT_POINT + torsion;
	assert!(!shifted.is_torsion_free());

	let bad_point = Point(shifted);
	let encoded = bincode::serialize(&bad_point).expect("serialize");

	// bincode is the wire format used by the multisig P2P layer
	// (see `ceremony_manager.rs` / `common/broadcast.rs`).
	let result: Result<Point, _> = bincode::deserialize(&encoded);

	assert!(
		result.is_err(),
		"BUG: deserialize accepted a torsion-shifted point. \
		 FROST requires the underlying group to be prime-order; \
		 a non-prime-order commitment shifts the aggregate pubkey \
		 into a non-prime-order coset and breaks the security proof.",
	);
}
