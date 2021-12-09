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
    // All used id's
    used_ids: HashSet<CeremonyId>,
    db_colum: u32,
    db: Arc<Mutex<S>>,
    logger: slog::Logger,
}

impl<S> CeremonyIdTracker<S>
where
    S: MultisigDB,
{
    /// Create a new `CeremonyIdTracker` and load the persistent information
    pub fn new(logger: slog::Logger, ceremony_id_db: Arc<Mutex<S>>, db_colum: u32) -> Self {
        let used_ids = ceremony_id_db
            .lock()
            .unwrap()
            .load_used_ceremony_ids(db_colum);
        CeremonyIdTracker {
            used_ids,
            db: ceremony_id_db,
            logger,
            db_colum,
        }
    }

    /// Mark this ceremony id as used
    pub fn consume_ceremony_id(&mut self, ceremony_id: &CeremonyId) {
        self.insert_used_ceremony_id(*ceremony_id);

        // Cleanup ceremonies that are more then `USED_CEREMONY_IDS_AGE_LIMIT` old.
        let old_ceremonies: Vec<CeremonyId> = self
            .used_ids
            .iter()
            .filter(|id| {
                if ceremony_id > &USED_CEREMONY_IDS_AGE_LIMIT {
                    *id < &(ceremony_id - USED_CEREMONY_IDS_AGE_LIMIT)
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        for id in old_ceremonies {
            self.remove_used_ceremony_id(&id);
        }
    }

    /// Check if the ceremony id has already been used (false = never seen before, safe to continue)
    pub fn is_ceremony_id_used(&self, ceremony_id: &CeremonyId) -> bool {
        self.used_ids.contains(ceremony_id)
    }

    fn remove_used_ceremony_id(&mut self, ceremony_id: &CeremonyId) {
        if !self.used_ids.remove(ceremony_id) {
            slog::warn!(
                self.logger,
                "Ceremony id tracking error: already consumed id {}",
                ceremony_id
            );
        }
        self.db
            .lock()
            .unwrap()
            .remove_used_ceremony_id(ceremony_id, self.db_colum);
    }

    fn insert_used_ceremony_id(&mut self, ceremony_id: CeremonyId) {
        self.used_ids.insert(ceremony_id);
        self.db
            .lock()
            .unwrap()
            .save_used_ceremony_id(ceremony_id, self.db_colum);
    }
}

#[test]
fn test_ceremony_id_tracker() {
    use crate::multisig::db::MultisigDBMock;

    let logger = crate::logging::test_utils::new_test_logger();

    let mut tracker =
        CeremonyIdTracker::new(logger, Arc::new(Mutex::new(MultisigDBMock::new())), 1);

    // Test the starting condition (starting from non-zero)
    assert!(!tracker.is_ceremony_id_used(&0));
    tracker.consume_ceremony_id(&10);
    assert!(tracker.is_ceremony_id_used(&10));
    assert!(!tracker.is_ceremony_id_used(&0));

    // Large set with a gap
    for i in 11..=99 {
        if i != 42 {
            tracker.consume_ceremony_id(&i);
        }
    }

    // Setting the lowest used id
    tracker.consume_ceremony_id(&5);

    assert!(!tracker.is_ceremony_id_used(&4));
    assert!(tracker.is_ceremony_id_used(&5));
    assert!(tracker.is_ceremony_id_used(&50));
    assert!(tracker.is_ceremony_id_used(&99));
    assert!(!tracker.is_ceremony_id_used(&42));
    assert!(!tracker.is_ceremony_id_used(&100));
}
