use std::collections::HashSet;

use pallet_cf_vaults::CeremonyId;

// Ids that are more then this amount behind the latest id are removed
const USED_CEREMONY_IDS_AGE_LIMIT: u64 = 1_000;

/// Used to track every ceremony id that has been used in the past,
/// so we can make sure they are not reused.
#[derive(Clone)]
pub struct CeremonyIdTracker {
    used_signing_ids: UsedCeremonyIds,
    used_keygen_ids: UsedCeremonyIds,
    logger: slog::Logger,
}

impl CeremonyIdTracker {
    // Create a new `CeremonyIdTracker` with empty `UsedCeremonyIds`
    pub fn new(logger: slog::Logger) -> Self {
        CeremonyIdTracker {
            used_signing_ids: UsedCeremonyIds {
                ids: HashSet::new(),
            },
            used_keygen_ids: UsedCeremonyIds {
                ids: HashSet::new(),
            },
            logger,
        }
    }

    /// Mark this ceremony id as used
    pub fn consume_signing_id(&mut self, ceremony_id: &CeremonyId) {
        self.used_signing_ids.add(ceremony_id);
    }

    /// Mark this ceremony id as used
    pub fn consume_keygen_id(&mut self, ceremony_id: &CeremonyId) {
        self.used_keygen_ids.add(ceremony_id);
    }

    /// Check if the ceremony id has already been used
    pub fn is_signing_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.used_signing_ids.is_used(ceremony_id)
    }

    /// Check if the ceremony id has already been used
    pub fn is_keygen_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.used_keygen_ids.is_used(ceremony_id)
    }
}

/// Wrapper around the used ceremony id data
#[derive(Clone)]
struct UsedCeremonyIds {
    // All used ids
    ids: HashSet<CeremonyId>,
}

impl UsedCeremonyIds {
    /// Mark this ceremony id as used
    pub fn add(&mut self, ceremony_id: &CeremonyId) {
        // Cleanup ceremonies that are more then `USED_CEREMONY_IDS_AGE_LIMIT` old.
        self.ids
            .retain(|id| *id > ceremony_id.saturating_sub(USED_CEREMONY_IDS_AGE_LIMIT));

        // Mark the ceremony id as used by adding it to the hashset
        self.ids.insert(*ceremony_id);
    }

    /// Check if the ceremony id has already been used (false = never seen before, safe to continue)
    pub fn is_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.ids.contains(ceremony_id)
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
    let mut used_ids = UsedCeremonyIds {
        ids: HashSet::new(),
    };

    // Consume an id and the id +1
    let test_id = 100;
    used_ids.add(&test_id);
    used_ids.add(&(test_id + 1));
    assert!(used_ids.is_used(&(test_id + 1)));
    assert!(used_ids.is_used(&test_id));

    // Now consume an id that is past the age limit for the id, but not the id+1
    used_ids.add(&(test_id + USED_CEREMONY_IDS_AGE_LIMIT));
    assert!(!used_ids.is_used(&test_id));
    assert!(used_ids.is_used(&(test_id + 1)));
}
