use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use pallet_cf_vaults::CeremonyId;

use crate::multisig::MultisigDB;

// Id's that are more then this amount behind the latest id are removed
const USED_CEREMONY_IDS_AGE_LIMIT: u64 = 1_000;

/// Used to track every ceremony id that has been used in the past,
/// so we can make sure they are not reused.
#[derive(Clone)]
pub struct CeremonyIdTracker<S>
where
    S: MultisigDB,
{
    used_signing_ids: UsedCeremonyIds,
    used_keygen_ids: UsedCeremonyIds,
    db: Arc<Mutex<S>>,
    logger: slog::Logger,
}

impl<S> CeremonyIdTracker<S>
where
    S: MultisigDB,
{
    // Create a new `CeremonyIdTracker` and load the persistent data from the db
    pub fn new(logger: slog::Logger, db: Arc<Mutex<S>>) -> Self {
        let used_signing_ids = UsedCeremonyIds {
            ids: db.lock().unwrap().load_tracking_for_signing(),
        };
        let used_keygen_ids = UsedCeremonyIds {
            ids: db.lock().unwrap().load_tracking_for_keygen(),
        };

        CeremonyIdTracker {
            used_signing_ids,
            used_keygen_ids,
            db,
            logger,
        }
    }

    /// Mark this ceremony id as used
    pub fn consume_signing_id(&mut self, ceremony_id: &CeremonyId) {
        self.used_signing_ids.add(ceremony_id);
        self.db
            .lock()
            .unwrap()
            .update_tracking_for_signing(&self.used_signing_ids.ids);
    }

    /// Mark this ceremony id as used
    pub fn consume_keygen_id(&mut self, ceremony_id: &CeremonyId) {
        self.used_keygen_ids.add(ceremony_id);
        self.db
            .lock()
            .unwrap()
            .update_tracking_for_keygen(&self.used_keygen_ids.ids);
    }

    /// Check if the ceremony id has already been used
    pub fn is_signing_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.used_signing_ids.check(ceremony_id)
    }

    /// Check if the ceremony id has already been used
    pub fn is_keygen_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.used_keygen_ids.check(ceremony_id)
    }
}

/// Wrapper around the used ceremony id data
#[derive(Clone)]
struct UsedCeremonyIds {
    // All used id's
    ids: HashSet<CeremonyId>,
}

impl UsedCeremonyIds {
    /// Mark this ceremony id as used
    pub fn add(&mut self, ceremony_id: &CeremonyId) {
        // Cleanup ceremonies that are more then `USED_CEREMONY_IDS_AGE_LIMIT` old.
        self.ids
            .retain(|id| *id < ceremony_id.saturating_sub(USED_CEREMONY_IDS_AGE_LIMIT));

        // Mark the ceremony id as used by adding it to the hashset
        self.ids.insert(*ceremony_id);
    }

    /// Check if the ceremony id has already been used (false = never seen before, safe to continue)
    pub fn check(&self, ceremony_id: &CeremonyId) -> bool {
        self.ids.contains(ceremony_id)
    }
}

// Test consuming an id marks it as used
#[test]
fn test_ceremony_id_consumption() {
    use crate::multisig::db::MultisigDBMock;

    let mut tracker = CeremonyIdTracker::new(
        crate::logging::test_utils::new_test_logger(),
        Arc::new(Mutex::new(MultisigDBMock::new())),
    );
    let test_id = 69;
    assert!(!tracker.is_signing_ceremony_id_used(&test_id));
    tracker.consume_signing_id(&test_id);
    assert!(tracker.is_signing_ceremony_id_used(&test_id));

    assert!(!tracker.is_keygen_ceremony_id_used(&test_id));
    tracker.consume_keygen_id(&test_id);
    assert!(tracker.is_keygen_ceremony_id_used(&test_id));
}

// Test that the age limit is enforced
#[test]
fn test_ceremony_id_age_limit() {
    use crate::multisig::db::MultisigDBMock;

    let mut tracker = CeremonyIdTracker::new(
        crate::logging::test_utils::new_test_logger(),
        Arc::new(Mutex::new(MultisigDBMock::new())),
    );

    let test_id = 69;
    tracker.consume_signing_id(&test_id);
    assert!(tracker.is_signing_ceremony_id_used(&test_id));
    tracker.consume_signing_id(&(test_id + USED_CEREMONY_IDS_AGE_LIMIT));
    assert!(!tracker.is_signing_ceremony_id_used(&test_id));

    tracker.consume_keygen_id(&test_id);
    assert!(tracker.is_keygen_ceremony_id_used(&test_id));
    tracker.consume_keygen_id(&(test_id + USED_CEREMONY_IDS_AGE_LIMIT));
    assert!(!tracker.is_keygen_ceremony_id_used(&test_id));
}
