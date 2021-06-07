// We want to re-export certain types here
// to make sure all of our dependencies on
// this module are in one place
mod bitcoin_schnorr;

pub(super) use bitcoin_schnorr::{
    KeyGenBroadcastMessage1, Keys, LocalSig, Parameters, SharedKeys, Signature,
};
