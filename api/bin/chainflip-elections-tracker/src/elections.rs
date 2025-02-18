use std::{collections::BTreeMap, fmt::Display};

use bitvec::prelude::*;
use codec::{Decode, Encode};
use pallet_cf_elections::{
	ElectionIdentifierOf, ElectoralSystemTypes, IndividualComponentOf, UniqueMonotonicIdentifier,
	electoral_system::BitmapComponentOf,
};

/// Since spans can only be submitted when they end, it is not possible to
/// have an infinitely running root span to which we attach all child spans.
/// Also, the trace view of Grafana becomes unreadable if we add more and more
/// as time goes on.
/// So what we do instead is that we split statechain blocks into 5 minute intervals,
/// and every 5 minutes create a new set of traces. Since statechain blocks have a frequency
/// of 6 seconds, we get that 50 blocks go into each trace.
const BLOCKS_PER_TRACE: u32 = 50;

#[derive(Debug, Eq, PartialEq, Clone, Encode, Decode)]
pub struct ElectionData<ES: ElectoralSystemTypes> {
	pub height: u32,

	#[allow(clippy::type_complexity)]
	pub bitmaps: BTreeMap<
		UniqueMonotonicIdentifier,
		Vec<(BitmapComponentOf<ES>, BitVec<u8, bitvec::order::Lsb0>)>,
	>,

	pub individual_components:
		BTreeMap<UniqueMonotonicIdentifier, BTreeMap<usize, IndividualComponentOf<ES>>>,

	pub elections: BTreeMap<ElectionIdentifierOf<ES>, (String, ES::ElectionProperties)>,

	pub electoral_system_names: Vec<String>,

	pub validators_count: u32,

	pub _phantom: std::marker::PhantomData<ES>,
}

// NOTE! the order is important for ordering the traces!
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Category {
	Properties,
	NoVote,
	Vote(String),
}
use crate::trace::StateTree;

use self::Category::*;

impl Display for Category {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			NoVote => write!(f, "Computing vote"),
			Vote(s) => write!(f, "Election unchanged: {s}"),
			Properties => write!(f, "New properties"),
		}
	}
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Key {
	RootBlockHeight(u32),
	ElectoralSystem(String),
	Election(String),
	Category(String, Category),
	Validator(u32),
	State { summary: String },
}

use Key::*;

impl Display for Key {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			RootBlockHeight(h) => write!(f, "blocks {h}-{}", h + BLOCKS_PER_TRACE - 1),
			Election(e) => write!(f, "{e}"),
			Key::Category(extra, category) => write!(f, "[{extra}] {category}"),
			Validator(x) => write!(f, "Validator {x}"),
			ElectoralSystem(name) => write!(f, "ES {name}"),
			State { summary } => write!(f, "{summary}"),
		}
	}
}

pub fn cloned_vec<'a, XS: IntoIterator<Item = &'a X>, X>(xs: XS) -> Vec<X>
where
	X: Clone + 'a,
{
	xs.into_iter().cloned().collect()
}

/// Initial value from which the trace state will be created
#[derive(Clone)]
pub struct TraceInit {
	pub end_immediately: bool,
	pub attributes: Vec<(String, String)>,
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

pub fn make_traces<ES: ElectoralSystemTypes>(data: ElectionData<ES>) -> StateTree<Key, TraceInit>
where
	IndividualComponentOf<ES>: Encode,
{
	let mut votes: BTreeMap<(ElectionIdentifierOf<ES>, u32), (String, String)> = BTreeMap::new();

	for (identifier, (_name, _properties)) in &data.elections {
		if let Some(bitmaps) = data.bitmaps.get(identifier.unique_monotonic()) {
			for (component, bitmap) in bitmaps {
				for (id, bit) in bitmap.iter().enumerate() {
					if *bit {
						votes.insert(
							(*identifier, id as u32),
							(AsHex(component.encode()).to_string(), format!("{component:?}")),
						);
					}
				}
			}
		}

		if let Some(individual_components) =
			data.individual_components.get(identifier.unique_monotonic())
		{
			for (authority_index, component) in individual_components {
				votes.insert(
					(*identifier, *authority_index as u32),
					(AsHex(component.encode()).to_string(), format!("{component:?}")),
				);
			}
		}
	}

	let end = TraceInit { end_immediately: true, attributes: Vec::new() };
	let start = TraceInit { end_immediately: false, attributes: Vec::new() };

	let mut trace = StateTree::new();

	let root_height = data.height - (data.height % BLOCKS_PER_TRACE);
	let key0 = RootBlockHeight(root_height);
	trace.insert(vec![key0.clone()], end.with_attribute("height".into(), format!("{root_height}")));

	for name in data.electoral_system_names {
		trace.insert(
			vec![key0.clone(), ElectoralSystem(name.clone())],
			end.with_attribute("height".into(), format!("{root_height}")),
		);
	}

	for (identifier, (name, properties)) in &data.elections {
		let input = identifier.encode();
		let mut other: &[u8] = &input;
		let id: u64 = Decode::decode(&mut other).unwrap();
		let extra = format!("{:?}", identifier.extra());

		let key1 = ElectoralSystem(name.clone());
		let key2 = Election(format!("{name} ({id})"));

		// election id
		trace.insert(cloned_vec([&key0, &key1, &key2]), end.clone());

		// properties
		let key3 = Category(extra.clone(), Properties);
		trace.insert(
			cloned_vec([&key0, &key1, &key2, &key3]),
			end.with_attribute("Properties".into(), format!("{properties:#?}")),
		);

		// votes and no-votes
		for authority_id in 0..data.validators_count {
			let (key, trace_init) = match votes.get(&(*identifier, authority_id)) {
				Some(s) => (
					Category(extra.clone(), Vote(s.0.clone())),
					start.with_attribute("vote".into(), s.1.clone()),
				),
				None => (Category(extra.clone(), NoVote), start.clone()),
			};

			trace.insert(cloned_vec([&key0, &key1, &key2, &key]), trace_init.clone());
			trace.insert(
				cloned_vec([&key0, &key1, &key2, &key3, &Validator(authority_id)]),
				trace_init,
			);
		}
	}

	trace
}
