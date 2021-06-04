mod test {
    use crate::*;
    use crate::{mock::*};
    use frame_support::{assert_ok};

    const ALICE: u64 = 100;
    const BOB: u64 = 101;

    #[test]
    fn report_them() {
        new_test_ext().execute_with(|| {
            // Our bad behaviour
            let bad_behaviour: Behaviour = Behaviour(vec![6, 6, 6]);
            // Add ALICE to our set of accounts we wish to monitor
            assert_ok!(AlivePallet::add_account(&ALICE));
            // Try to add ALICE again will return an error
            assert_eq!(AlivePallet::add_account(&ALICE).unwrap_err(),
                       JudgementError::AccountExists);
            // Try to report on BOB won't be tolerated
            assert_eq!(AlivePallet::report(&BOB, bad_behaviour.clone()).unwrap_err(),
                       JudgementError::AccountNotFound);
            // Report on ALICE
            assert_ok!(AlivePallet::report(&ALICE, bad_behaviour.clone()));
            // Get report on ALICE
            let report = AlivePallet::report_for(&ALICE).unwrap();
            assert!(report.len() == 1);
            assert!(report[0] == bad_behaviour);
            // Get liveliness on ALICE, should be block 1
            let liveliness = AlivePallet::liveliness(&ALICE).unwrap();
            assert_eq!(liveliness, 1);
            // Run to block 10 and report and check liveliness
            run_to_block(10);
            assert_ok!(AlivePallet::report(&ALICE, bad_behaviour.clone()));
            let liveliness = AlivePallet::liveliness(&ALICE).unwrap();
            assert_eq!(liveliness, 10);
            // Fail to clean report for BOB
            assert_eq!(AlivePallet::clean_all(&BOB).unwrap_err(), JudgementError::AccountNotFound);
            // Clear report on ALICE
            assert_ok!(AlivePallet::clean_all(&ALICE));
            let report = AlivePallet::report_for(&ALICE).unwrap();
            assert_eq!(report.len(), 0);
            // Fail to get a report on BOB
            assert_eq!(AlivePallet::report_for(&BOB).unwrap_err(), JudgementError::AccountNotFound);
            // Remove account for ALICE
            assert_ok!(AlivePallet::remove_account(&ALICE));
            // Try to remove the account for BOB and fail
            assert_eq!(AlivePallet::remove_account(&BOB).unwrap_err(),
                       JudgementError::AccountNotFound);
        });
    }
}