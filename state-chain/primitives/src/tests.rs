//! Provides tests for primitive types implementations
use crate::SemVer;
use sp_std::cmp::Ordering;

#[test]
fn ord_works_for_semver() {
	let target = SemVer { major: 1, minor: 5, patch: 8 };

	assert!(SemVer { major: 0, minor: 5, patch: 8 } < target);
	assert!(SemVer { major: 1, minor: 4, patch: 8 } < target);
	assert!(SemVer { major: 1, minor: 5, patch: 7 } < target);

	assert_eq!(SemVer { major: 1, minor: 5, patch: 8 }.cmp(&target), sp_std::cmp::Ordering::Equal);

	assert!(SemVer { major: 2, minor: 5, patch: 8 } > target);
	assert!(SemVer { major: 1, minor: 6, patch: 8 } > target);
	assert!(SemVer { major: 1, minor: 5, patch: 9 } > target);
}

#[test]
fn cmp_ignore_patch_works_for_semver() {
	let target = SemVer { major: 1, minor: 5, patch: 8 };

	assert_eq!(SemVer { major: 0, minor: 5, patch: 8 }.cmp_ignore_patch(&target), Ordering::Less);
	assert_eq!(SemVer { major: 1, minor: 4, patch: 8 }.cmp_ignore_patch(&target), Ordering::Less);

	assert_eq!(SemVer { major: 1, minor: 5, patch: 7 }.cmp_ignore_patch(&target), Ordering::Equal);
	assert_eq!(SemVer { major: 1, minor: 5, patch: 9 }.cmp_ignore_patch(&target), Ordering::Equal);

	assert_eq!(
		SemVer { major: 2, minor: 5, patch: 8 }.cmp_ignore_patch(&target),
		Ordering::Greater
	);
	assert_eq!(
		SemVer { major: 1, minor: 6, patch: 8 }.cmp_ignore_patch(&target),
		Ordering::Greater
	);
}

#[test]
fn semver_is_compatible_works() {
	let target = SemVer { major: 1, minor: 5, patch: 8 };

	assert!(!SemVer { major: 0, minor: 5, patch: 8 }.is_compatible(&target));
	assert!(!SemVer { major: 1, minor: 4, patch: 8 }.is_compatible(&target));

	assert!(SemVer { major: 1, minor: 5, patch: 7 }.is_compatible(&target));
	assert!(SemVer { major: 1, minor: 5, patch: 9 }.is_compatible(&target));

	assert!(SemVer { major: 2, minor: 5, patch: 8 }.is_compatible(&target));
	assert!(SemVer { major: 1, minor: 6, patch: 8 }.is_compatible(&target));
}
