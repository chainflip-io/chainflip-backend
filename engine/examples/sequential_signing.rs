use chainflip_engine::signing::crypto::{
    KeyShare, Keys, LegacySignature, LocalSig, Parameters, VerifiableSS, FE, GE,
};

pub fn keygen_t_n_parties(
    t: usize,
    n: usize,
    parties: &[usize],
) -> (Vec<Keys>, Vec<KeyShare>, GE, Vec<VerifiableSS<GE>>) {
    let params = Parameters {
        threshold: t,
        share_count: n,
    };
    // >>> Each party generates a key (privately)

    let party_keys_vec: Vec<Keys> = (0..n).map(|i| Keys::phase1_create(parties[i])).collect();

    let mut bc1_vec = Vec::new();
    let mut blind_vec = Vec::new();
    for i in 0..n.clone() {
        let (bc1, blind) = party_keys_vec[i].phase1_broadcast();
        bc1_vec.push(bc1);
        blind_vec.push(blind);
    }

    let y_vec = (0..n.clone())
        .map(|i| party_keys_vec[i].y_i.clone())
        .collect::<Vec<GE>>();

    let mut y_vec_iter = y_vec.iter();
    let head = y_vec_iter.next().unwrap();
    let tail = y_vec_iter;
    let y_sum = tail.fold(head.clone(), |acc, x| acc + x);

    let mut vss_scheme_vec = Vec::new();
    let mut secret_shares_vec = Vec::new();
    let mut index_vec = Vec::new();
    for i in 0..n.clone() {
        let (vss_scheme, secret_shares, index) = party_keys_vec[i]
            .phase1_verify_com_phase2_distribute(&params, &blind_vec, &y_vec, &bc1_vec, parties)
            .expect("invalid key");
        vss_scheme_vec.push(vss_scheme);
        secret_shares_vec.push(secret_shares);
        index_vec.push(index);
    }

    // For each player i, collect shares distributed by players j
    let party_shares = (0..n.clone())
        .map(|i| {
            (0..n.clone())
                .map(|j| {
                    let vec_j = &secret_shares_vec[j];
                    vec_j[i].clone()
                })
                .collect::<Vec<FE>>()
        })
        .collect::<Vec<Vec<FE>>>();

    let mut shared_keys_vec = Vec::new();
    for i in 0..n.clone() {
        let (shared_keys, _) = party_keys_vec[i]
            .phase2_verify_vss_construct_keypair(
                &params,
                &y_vec,
                &party_shares[i],
                &vss_scheme_vec,
                &index_vec[i],
            )
            .expect("invalid vss");
        shared_keys_vec.push(shared_keys);
    }

    // y_sum is the shared public key
    (party_keys_vec, shared_keys_vec, y_sum, vss_scheme_vec)
}

fn sign(
    message: &[u8],
    t: usize,
    eph_shared_keys_vec: &Vec<KeyShare>,
    priv_shared_keys_vec: &Vec<KeyShare>,
    parties_index_vec: &[usize],
    key_gen_vss_vec: &Vec<VerifiableSS<GE>>,
    eph_vss_vec: &Vec<VerifiableSS<GE>>,
    v: GE,
    y: GE,
) {
    // each party computes and share a local sig, we collected them here to a vector as each party should do AFTER receiving all local sigs
    let local_sig_vec = (0..t.clone())
        .map(|i| {
            LocalSig::compute(
                &message,
                &eph_shared_keys_vec[i],
                &priv_shared_keys_vec[parties_index_vec[i]],
            )
        })
        .collect::<Vec<LocalSig>>();

    let verify_local_sig = LocalSig::verify_local_sigs(
        &local_sig_vec,
        &parties_index_vec,
        &key_gen_vss_vec,
        &eph_vss_vec,
    );

    assert!(verify_local_sig.is_ok());
    let vss_sum_local_sigs = verify_local_sig.unwrap();

    // each party / dealer can generate the signature
    let signature =
        LegacySignature::generate(&vss_sum_local_sigs, &local_sig_vec, &parties_index_vec, v);
    let verify_sig = signature.verify(&message, &y);
    if verify_sig.is_ok() {
        println!("Generated signature is OK!");
    }
}

#[allow(non_snake_case)]
fn main() {
    let t = 1;
    let n = 3;

    let key_gen_parties_index_vec: [usize; 3] = [0, 1, 3];
    let key_gen_parties_points_vec = (0..key_gen_parties_index_vec.len())
        .map(|i| key_gen_parties_index_vec[i].clone() + 1)
        .collect::<Vec<usize>>();

    let parties = &key_gen_parties_points_vec;

    let (_priv_keys_vec, priv_shared_keys_vec, Y, key_gen_vss_vec) =
        keygen_t_n_parties(t, n, parties);

    println!("Generated multisig key");

    // signing

    // Generate a random shared secret: (e, V, c_iG)
    let parties_index_vec: [usize; 2] = [0, 1];
    let parties_points_vec = (0..parties_index_vec.len())
        .map(|i| parties_index_vec[i].clone() + 1)
        .collect::<Vec<usize>>();
    let num_parties = parties_index_vec.len();
    let (_eph_keys_vec, eph_shared_keys_vec, V, eph_vss_vec) =
        keygen_t_n_parties(t.clone(), num_parties.clone(), &parties_points_vec);

    let message: [u8; 4] = [79, 77, 69, 82];

    // Note that "private" keys are not used here (unlike the private components of the shared key)
    sign(
        &message,
        t + 1,
        &eph_shared_keys_vec,
        &priv_shared_keys_vec,
        &parties_index_vec,
        &key_gen_vss_vec,
        &eph_vss_vec,
        V,
        Y,
    );
}
