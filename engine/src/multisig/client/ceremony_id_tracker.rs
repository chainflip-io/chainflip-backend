use std::collections::HashSet;

use pallet_cf_vaults::CeremonyId;

// Ids that are more then this amount behind the latest id are removed
const TRACKED_CEREMONY_AGE_LIMIT: u64 = 1_000;

/// Enum used internally to track signing and keygen ceremony ids.
/// They are tracked separately so they can have overlapping CeremonyId's.
#[derive(Clone, PartialEq, Eq, Hash)]
enum TrackedCeremony {
    Keygen(CeremonyId),
    Signing(CeremonyId),
}

/// Used to track every ceremony id that has been used in the past,
/// so we can make sure they are not reused.
#[derive(Clone)]
pub struct CeremonyIdTracker {
    used_ceremonies: HashSet<TrackedCeremony>,
    logger: slog::Logger,
}

impl CeremonyIdTracker {
    // Create a new `CeremonyIdTracker` with empty `UsedCeremonyIds`
    pub fn new(logger: slog::Logger) -> Self {
        CeremonyIdTracker {
            used_ceremonies: HashSet::new(),
            logger,
        }
    }

    /// Mark this signing ceremony id as used
    pub fn consume_signing_id(&mut self, ceremony_id: &CeremonyId) {
        self.consume_ceremony(&TrackedCeremony::Signing(*ceremony_id));
    }

    /// Mark this keygen ceremony id as used
    pub fn consume_keygen_id(&mut self, ceremony_id: &CeremonyId) {
        self.consume_ceremony(&TrackedCeremony::Keygen(*ceremony_id));
    }

    fn consume_ceremony(&mut self, ceremony_to_consume: &TrackedCeremony) {
        // Cleanup ceremonies that are more then `TRACKED_CEREMONY_AGE_LIMIT` old.
        let ceremony_id = match ceremony_to_consume {
            TrackedCeremony::Signing(id) => id,
            TrackedCeremony::Keygen(id) => id,
        };
        self.used_ceremonies.retain(|used_id_number| {
            let used_id = match used_id_number {
                TrackedCeremony::Signing(id) => id,
                TrackedCeremony::Keygen(id) => id,
            };
            *used_id > ceremony_id.saturating_sub(TRACKED_CEREMONY_AGE_LIMIT)
        });

        // Mark the ceremony id as used by adding it to the hashset
        self.used_ceremonies.insert(ceremony_to_consume.clone());
    }

    /// Check if the ceremony id has already been used for singing
    pub fn is_signing_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.used_ceremonies
            .contains(&TrackedCeremony::Signing(*ceremony_id))
    }

    /// Check if the ceremony id has already been used for keygen
    pub fn is_keygen_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.used_ceremonies
            .contains(&TrackedCeremony::Keygen(*ceremony_id))
    }
}

// Test consuming an id marks it as used
#[test]
fn test_ceremony_id_consumption() {
    let mut tracker = CeremonyIdTracker::new(crate::logging::test_utils::new_test_logger());

    // Using a different id for signing & keygen, to confirm no cross contamination
    let signing_test_id = 1;
    let keygen_test_id = 2;
    assert_ne!(signing_test_id, keygen_test_id);

    assert!(!tracker.is_signing_ceremony_id_used(&signing_test_id));
    tracker.consume_signing_id(&signing_test_id);
    assert!(tracker.is_signing_ceremony_id_used(&signing_test_id));
    assert!(!tracker.is_keygen_ceremony_id_used(&signing_test_id));

    assert!(!tracker.is_keygen_ceremony_id_used(&keygen_test_id));
    tracker.consume_keygen_id(&keygen_test_id);
    assert!(tracker.is_keygen_ceremony_id_used(&keygen_test_id));
    assert!(!tracker.is_signing_ceremony_id_used(&keygen_test_id));
}

// Test that the age limit is enforced
#[test]
fn test_ceremony_id_age_limit() {
    let mut tracker = CeremonyIdTracker::new(crate::logging::test_utils::new_test_logger());
    let signing_test_id = 1;
    let keygen_test_id = 2;
    assert_ne!(signing_test_id, keygen_test_id);

    // Consume an id and the id +1
    tracker.consume_signing_id(&signing_test_id);
    tracker.consume_signing_id(&(signing_test_id + 1));
    assert!(tracker.is_signing_ceremony_id_used(&signing_test_id));
    assert!(tracker.is_signing_ceremony_id_used(&(signing_test_id + 1)));

    tracker.consume_keygen_id(&keygen_test_id);
    tracker.consume_keygen_id(&(keygen_test_id + 1));
    assert!(tracker.is_keygen_ceremony_id_used(&keygen_test_id));
    assert!(tracker.is_keygen_ceremony_id_used(&(keygen_test_id + 1)));

    // Now consume an id that is past the age limit for the id, but not the id+1
    tracker.consume_signing_id(&(signing_test_id + TRACKED_CEREMONY_AGE_LIMIT));
    assert!(!tracker.is_signing_ceremony_id_used(&signing_test_id));
    assert!(tracker.is_signing_ceremony_id_used(&(signing_test_id + 1)));

    tracker.consume_keygen_id(&(keygen_test_id + TRACKED_CEREMONY_AGE_LIMIT));
    assert!(!tracker.is_keygen_ceremony_id_used(&keygen_test_id));
    assert!(tracker.is_keygen_ceremony_id_used(&(keygen_test_id + 1)));
}
