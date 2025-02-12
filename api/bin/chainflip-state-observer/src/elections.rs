
use std::{collections::BTreeMap, fmt::Display, hash::{DefaultHasher, Hash, Hasher}};

use crate::{trace::Trace, ElectionData};
use codec::{Decode, Encode};
use pallet_cf_elections::{bitmap_components::ElectionBitmapComponents, electoral_system::BitmapComponentOf, vote_storage::VoteStorage, ElectionIdentifierOf, ElectoralSystemTypes, IndividualComponentOf, SharedDataHash, UniqueMonotonicIdentifier};
use bitvec::prelude::*;

// pub struct ElectionData<ES: ElectoralSystemTypes> {
//     properties: ES::ElectionProperties,
//     validators: Vec<ES::ValidatorId>,
//     shared_votes: BTreeMap<SharedDataHash, <ES::VoteStorage as VoteStorage>::SharedData>,
//     bitmaps: Vec<(BitmapComponentOf<ES>, BitVec<u8, bitvec::order::Lsb0>)>,

//     _phantom: std::marker::PhantomData<ES>
// }

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Category {
    NoVote,
    BitmapVote(String),
    IndividualVote(String),
    PartialVote(String),
    Properties
}
use self::Category::*;

impl Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoVote => write!(f, "No Vote"),
            BitmapVote(s) => write!(f, "Bitmap: {s}"),
            IndividualVote(s) => write!(f, "Individual: {s}"),
            PartialVote(s) => write!(f, "Partial: {s}"),
            Properties => write!(f, "Properties"),
        }
    }
}


#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Key {
    ElectoralSystem(String),
    Election(String),
    Category(Category),
    Validator(u64),
    State{summary: String},
}

impl Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Election(e) => write!(f, "{e}"),
            Key::Category(category) => write!(f, "{category}"),
            Validator(x) => write!(f, "Validator {x}"),
            ElectoralSystem(name) => write!(f, "ES {name}"),
            State { summary } => write!(f, "{summary}"),
        }
    }
}

use Key::*;

pub fn cloned_vec<'a, XS: IntoIterator<Item = &'a X>, X>(xs: XS) -> Vec<X>
where X : Clone + 'a
{
    xs.into_iter().cloned().collect()
}

/// Initial value from which the trace state will be created
#[derive(Clone)]
pub struct TraceInit {
    pub end_immediately: bool,
    pub values: Vec<(String, String)>
}

impl TraceInit {
    pub fn with_value(&self, key: String, value: String) -> Self {
        let mut result = self.clone();
        result.values.push((key, value));
        result
    }
}

pub fn make_traces<ES: ElectoralSystemTypes>(data: ElectionData<ES>) -> Trace<Key, TraceInit> 
where IndividualComponentOf<ES>: Encode
{
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

    let end = TraceInit {
        end_immediately: true,
        values: Vec::new()
    };
    let start = TraceInit {
        end_immediately: false,
        values: Vec::new()
    };

    let mut trace = Trace::new();
    trace.insert(vec![], end.clone());

    for (identifier, (name, properties)) in &data.election_names {

    // } 
    // for (k, bitmaps) in data.bitmaps {
        // let name = data.election_names.get(&k).cloned().unwrap_or(format!("{k:?}"));
        let input = identifier.encode();
        let mut other: &[u8] = &input;
        let id: u64 = Decode::decode(&mut other).unwrap();

        let key0 = ElectoralSystem(name.clone());
        let key1 = Election(format!("{name} ({id})"));

        // general
        trace.insert(cloned_vec([&key0]), end.clone());
        trace.insert(cloned_vec([&key0, &key1]), end.clone());

        // properties
        let key2 = Category(Properties);
        trace.insert(cloned_vec([&key0, &key1, &key2]), end.clone());
        trace.insert(
            cloned_vec([&key0, &key1, &key2, &State { summary: "new_properties".into() }]), 
            start.with_value("properties".into(), format!("{properties:?}"))
        );

        // bitmaps
        if let Some(bitmaps) = data.bitmaps.get(identifier) {

            for (component, bitmap) in bitmaps {
                let key2 = Category(BitmapVote("vote".into()));
                trace.insert(cloned_vec([&key0, &key1, &key2]), start.clone());
                for (id, bit) in bitmap.iter().enumerate() {
                    let key3 = Validator(id as u64);
                    if *bit {
                        trace.insert(cloned_vec([&key0, &key1, &key2, &key3]), start.clone());
                    }
                }
            }

        }

        // components
        if let Some(individual_components) = data.individual_components.get(identifier) {
            for (authority_index, component) in individual_components {
                // let mut hasher = DefaultHasher::new();
                let x = component.encode();
                // let result = hasher.finish();
                trace.insert(cloned_vec([&key0, &key1, &Category(IndividualVote(format!("{x:x?}")))]), start.clone());
                trace.insert(cloned_vec([&key0, &key1, &Category(IndividualVote(format!("{x:x?}"))), &Validator(*authority_index as u64)]), start.clone());
            }
        }

    }

    trace

    // Trace::Composite((), 
    //     data.bitmaps
    //         .into_iter()
    //         .map(|(k,bitmaps)| (Election(k),
    //             Trace::Composite((),
    //                  bitmaps
    //                 .iter()
    //                 .map(|(component, bitmap)| 
    //                     (
    //                         Category(FullVote("vote".into())),
    //                         Trace::Composite((), bitmap.iter().enumerate().map(|(id, bit)| (Validator(id as u64), Trace::Composite((), BTreeMap::new()))).collect())
    //                     )
    //                 )
    //                 .collect()
    //             )
    //         ))
    //         .collect()
    // )
}

// pub fn all_traces<ES: ElectoralSystemTypes + Ord>(data: BTreeMap<ElectionIdentifierOf<ES>,ElectionData<ES>>) -> Trace<Key<ES>, ()> {
// }
