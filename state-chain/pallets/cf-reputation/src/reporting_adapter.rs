//! # Offence Reporting Adapter for substrate offences.
//!
//! This module defines an adapter to allow substrate offences to be reported via our own
//! reporting/reputation framework.
//!
//! Hence in this module we simply define [ReportOffence].
use crate::*;
use cf_traits::offence_reporting::OffenceReporter;
use codec::Encode;
use frame_support::{traits::OnKilledAccount, Blake2_128Concat, StorageHasher};
use sp_staking::offence::ReportOffence;

pub type ReportId = <Blake2_128Concat as StorageHasher>::Output;
pub type OpaqueTimeSlot = Vec<u8>;

/// Note: `FullIdentification` is misleading since it actually only forms part of the 'full'
/// `IdentificationTuple`, but the naming has been preserved here for consistency with other
/// substrate pallets, in particular session_historical.
type IdentificationTuple<T, FullIdentification> =
	(<T as Chainflip>::ValidatorId, FullIdentification);

/// An adapter struct on which to implement [ReportOffence] for our runtime.
///
/// `FullIdentification` is the additional identification information used in the
/// `IdentificationTuple` of the [KeyOwnerProofSystem](frame_support::traits::KeyOwnerProofSystem)
/// used in the runtime, for the [sp_staking::offence::Offence] `O` being reported. Typically, the
/// [KeyOwnerProofSystem] is implemented in `pallet_session_historical`, and `FullIdentification =
/// ()`.
pub struct ChainflipOffenceReportingAdapter<T, O, FullIdentification>(
	sp_std::marker::PhantomData<(T, O, FullIdentification)>,
);

impl<T, O, FullIdentification> ChainflipOffenceReportingAdapter<T, O, FullIdentification>
where
	T: Config,
	O: sp_staking::offence::Offence<IdentificationTuple<T, FullIdentification>>,
{
	/// Encodes a unique hash for a tuple of (report_type, offender).
	///
	/// Since `O` is a trait object, we can't use the report ID tuple as a storage key directly. We
	/// need to explicity encode the key we want to use.
	fn report_id(offender: &T::ValidatorId) -> ReportId {
		(O::ID, offender).using_encoded(<Twox64Concat as StorageHasher>::hash)
	}

	/// Returns true iff, for this offender, we have already recorded another offence with the
	/// current or a later time slot.
	fn is_time_slot_stale(offender: &T::ValidatorId, time_slot: &O::TimeSlot) -> bool {
		OffenceTimeSlotTracker::<T>::get(Self::report_id(offender))
			.and_then(|bytes| O::TimeSlot::decode(&mut &bytes[..]).ok())
			.map(|last_reported_time_slot| time_slot <= &last_reported_time_slot)
			.unwrap_or_default()
	}
}

impl<T, O, FullIdentification>
	ReportOffence<T::ValidatorId, IdentificationTuple<T, FullIdentification>, O>
	for ChainflipOffenceReportingAdapter<T, O, FullIdentification>
where
	T: Config,
	O: sp_staking::offence::Offence<IdentificationTuple<T, FullIdentification>> + Into<T::Offence>,
{
	/// Reports a substrate offence.
	///
	/// This implementation assumes that the reporting authority is irrelevant, and will always be
	/// the current block author. This assumption holds for our runtime since we don't allow
	/// unsolicited offence reports (ie. there is no 'fisherman' role is with polkadot).
	///
	/// Another assumption is that reports are submitted for a single offender only. This assumption
	/// holds for GRANDPA but would have to be verified for any other components that wish to
	/// use this function.
	fn report_offence(
		_reporters: Vec<T::ValidatorId>,
		offence: O,
	) -> Result<(), sp_staking::offence::OffenceError> {
		if !T::SafeMode::get().reporting_enabled {
			return Ok(())
		}
		const CF_ERROR_EXPECTED_SINGLE_OFFENDER: u8 = 0xcf;

		let offenders = offence.offenders();
		ensure!(offenders.len() == 1, {
			log::warn!(
				"Offence report {:?} received for multiple offenders: this is unsupported.",
				O::ID
			);
			sp_staking::offence::OffenceError::Other(CF_ERROR_EXPECTED_SINGLE_OFFENDER)
		});
		let (offender, _) = offence.offenders().pop().expect("len == 1; qed");

		ensure!(
			!Self::is_time_slot_stale(&offender, &offence.time_slot()),
			sp_staking::offence::OffenceError::DuplicateReport
		);

		OffenceTimeSlotTracker::<T>::insert(
			Self::report_id(&offender),
			offence.time_slot().encode(),
		);

		// TODO: Reconsider the slashing rate here. For now we assume we are reporting the node
		// for equivocation, and that each report corresponds to equivocation on a single block.
		T::Slasher::slash(&offender, 1u32.into());

		Pallet::<T>::report(offence, offender);
		Ok(())
	}

	/// Checks if we have already seen this offence. Needs to be efficient since it's used in the
	/// mempool for transaction validity checks.
	///
	/// This implementation assumes that it's not possible to submit a report for a *future* time
	/// slot. Hence we can simply check if the reported slot is not later than the latest one seen
	/// for this offender.
	fn is_known_offence(
		offenders: &[IdentificationTuple<T, FullIdentification>],
		time_slot: &O::TimeSlot,
	) -> bool {
		offenders
			.iter()
			.any(|(offender, _)| Self::is_time_slot_stale(offender, time_slot))
	}
}

impl<T, O, FullIdentification> OnKilledAccount<T::ValidatorId>
	for ChainflipOffenceReportingAdapter<T, O, FullIdentification>
where
	T: Config,
	O: sp_staking::offence::Offence<IdentificationTuple<T, FullIdentification>>,
{
	fn on_killed_account(who: &T::ValidatorId) {
		OffenceTimeSlotTracker::<T>::remove(Self::report_id(who));
	}
}
