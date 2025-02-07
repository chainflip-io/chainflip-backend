
use std::collections::BTreeMap;

use crate::trace::Trace;
use pallet_cf_elections::{bitmap_components::ElectionBitmapComponents, electoral_system::{BitmapComponentOf, ElectionData}, vote_storage::VoteStorage, ElectionIdentifierOf, ElectoralSystemTypes, SharedDataHash, UniqueMonotonicIdentifier};
use bitvec::prelude::*;

// pub struct ElectionData<ES: ElectoralSystemTypes> {
//     properties: ES::ElectionProperties,
//     validators: Vec<ES::ValidatorId>,
//     shared_votes: BTreeMap<SharedDataHash, <ES::VoteStorage as VoteStorage>::SharedData>,
//     bitmaps: Vec<(BitmapComponentOf<ES>, BitVec<u8, bitvec::order::Lsb0>)>,

//     _phantom: std::marker::PhantomData<ES>
// }

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    NoVote,
    FullVote(String),
    PartialVote(String)
}
use self::Category::*;


#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Election(UniqueMonotonicIdentifier),
    Category(Category),
    Validator(u64)
}

use Key::*;

pub fn traces<ES: ElectoralSystemTypes>(data: ElectionData<ES>) -> Trace<Key, ()> {
    // let full_votes = data.votes.
    // let votes = data.bitmaps
    //     .iter()
    //     .map(|(component, bitmap)| 
    //         (
    //             Category(FullVote("vote".into())),
    //             Trace::Composite((), bitmap.iter().enumerate().map(|(id, bit)| (Validator(id as u64), Trace::Single(()))).collect())
    //         )
    //     )
    //     .collect();
    // Trace::Composite((), votes)

    Trace::Composite((), 
        data.bitmaps
            .into_iter()
            .map(|(k,bitmaps)| (Election(k),
                Trace::Composite((),
                     bitmaps
                    .iter()
                    .map(|(component, bitmap)| 
                        (
                            Category(FullVote("vote".into())),
                            Trace::Composite((), bitmap.iter().enumerate().map(|(id, bit)| (Validator(id as u64), Trace::Composite((), BTreeMap::new()))).collect())
                        )
                    )
                    .collect()
                )
            ))
            .collect()
    )
}

// pub fn all_traces<ES: ElectoralSystemTypes + Ord>(data: BTreeMap<ElectionIdentifierOf<ES>,ElectionData<ES>>) -> Trace<Key<ES>, ()> {
// }
