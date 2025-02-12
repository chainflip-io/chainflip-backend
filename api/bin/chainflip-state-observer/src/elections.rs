
use std::{collections::BTreeMap, fmt::{format, Display}, hash::{DefaultHasher, Hash, Hasher}};

use crate::{trace::Trace, ElectionData};
use codec::{Decode, Encode};
use pallet_cf_elections::{bitmap_components::ElectionBitmapComponents, electoral_system::BitmapComponentOf, vote_storage::VoteStorage, ElectionIdentifierOf, ElectoralSystemTypes, IndividualComponentOf, SharedDataHash, UniqueMonotonicIdentifier};
use bitvec::prelude::*;


// NOTE! the order is important for ordering the traces!
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Category {
    Properties,
    NoVote,
    Vote(String),
}
use self::Category::*;

impl Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoVote => write!(f, "No Vote"),
            Vote(s) => write!(f, "Vote: {s}"),
            Properties => write!(f, "Properties"),
            // IndividualVote(s) => write!(f, "Individual: {s}"),
            // PartialVote(s) => write!(f, "Partial: {s}"),
        }
    }
}


#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Key {
    ElectoralSystem(String),
    Election(String),
    Category(String, Category),
    Validator(u32),
    State{summary: String},
}

impl Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Election(e) => write!(f, "{e}"),
            Key::Category(extra, category) => write!(f, "[{extra}] {category}"),
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
    pub attributes: Vec<(String, String)>
}

impl TraceInit {
    pub fn with_attribute(&self, key: String, value: String) -> Self {
        let mut result = self.clone();
        result.attributes.push((key, value));
        result
    }
}


struct AsHex(Vec<u8>);

impl Display for AsHex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{b:x}")?;
        }
        Ok(())
    }
}

pub fn make_traces<ES: ElectoralSystemTypes>(data: ElectionData<ES>) -> Trace<Key, TraceInit> 
where IndividualComponentOf<ES>: Encode
{

    let mut votes: BTreeMap<(ElectionIdentifierOf<ES>, u32), String> = BTreeMap::new();

    for (identifier, (name, properties)) in &data.elections {

        if let Some(bitmaps) = data.bitmaps.get(identifier.unique_monotonic()) {

            for (component, bitmap) in bitmaps {
                for (id, bit) in bitmap.iter().enumerate() {
                    let key3 = Validator(id as u32);
                    if *bit {
                        votes.insert((*identifier, id as u32), AsHex(component.encode()).to_string());
                    }
                }
            }
        }

        if let Some(individual_components) = data.individual_components.get(identifier.unique_monotonic()) {
            for (authority_index, component) in individual_components {
                votes.insert((*identifier, *authority_index as u32), AsHex(component.encode()).to_string());
            }
        }
    }

    let end = TraceInit {
        end_immediately: true,
        attributes: Vec::new()
    };
    let start = TraceInit {
        end_immediately: false,
        attributes: Vec::new()
    };

    let mut trace = Trace::new();
    trace.insert(vec![], end.clone());

    for name in data.electoral_system_names {
        trace.insert(vec![ElectoralSystem(name.clone())], end.clone());
    }

    for (identifier, (name, properties)) in &data.elections {

        let input = identifier.encode();
        let mut other: &[u8] = &input;
        let id: u64 = Decode::decode(&mut other).unwrap();
        let extra = format!("{:?}", identifier.extra());

        let key0 = ElectoralSystem(name.clone());
        let key1 = Election(format!("{name} ({id})"));

        // election id
        trace.insert(cloned_vec([&key0, &key1]), end.clone());

        // properties
        let key2 = Category(extra.clone(), Properties);
        trace.insert(cloned_vec([&key0, &key1, &key2]), end.with_attribute("Properties".into(), format!("{properties:#?}")));

        // no votes
        for authority_id in 0..data.validators {
            if votes.get(&(*identifier, authority_id)).is_none() {
                trace.insert(cloned_vec([&key0, &key1, &Category(extra.clone(), NoVote)]), start.clone());
                trace.insert(cloned_vec([&key0, &key1, &Category(extra.clone(), NoVote), &Validator(authority_id)]), start.clone());
            }
        }

        // votes
        for authority_id in 0..data.validators {
            if let Some(s) = votes.get(&(*identifier, authority_id)) {
                trace.insert(cloned_vec([&key0, &key1, &Category(extra.clone(), Vote(s.clone()))]), start.clone());
                trace.insert(cloned_vec([&key0, &key1, &Category(extra.clone(), Vote(s.clone())), &Validator(authority_id)]), start.clone());
            }
        }

    }

    trace

}

