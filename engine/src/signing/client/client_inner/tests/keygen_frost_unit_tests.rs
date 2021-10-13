use super::helpers::check_blamed_paries;
use super::*;

macro_rules! receive_comm1 {
    ($client:expr, $sender: expr, $keygen_states:expr) => {
        let comm1 = $keygen_states.comm_stage1.comm1_vec[$sender].clone();
        let m = helpers::keygen_data_to_p2p(comm1, &VALIDATOR_IDS[$sender], KEYGEN_CEREMONY_ID);
        $client.process_p2p_message(m);
    };
}

// If keygen state expires before a formal request to keygen
// (from our SC), we should report initiators of that ceremony
#[tokio::test]
async fn report_initiators_of_unexpected_keygen() {
    let mut ctx = helpers::KeygenContext::new();
    let keygen_states = ctx.generate().await;

    let mut c1 = keygen_states.stage0.clients[0].clone();

    let bad_party_idx = 1;

    receive_comm1!(c1, bad_party_idx, keygen_states);

    // Force all ceremonies to time out
    c1.expire_all();
    c1.cleanup();

    check_blamed_paries(&mut ctx.rxs[0], &[bad_party_idx]).await;
}
