//! This file defines a proptest strategy for generating a sequence of states of a progressing
//! blockchain. The blocks of the chain contain vectors of an arbitrary datatype. The generated
//! sequence of states includes reorgs (block data may change within a given safety margin) and
//! "stuttering" / where the chain state may remain unchanged or even revert to a previous state for
//! a short time (modelling network delay and difference of the chainview when accessing from
//! different rpc endpoints).

use core::{
	fmt::Debug,
	ops::{Range, RangeInclusive},
};

use cf_chains::witness_period::{BlockZero, SaturatingStep};
use proptest::{collection::*, prelude::*};

/// The basic data structure involved is an ordered tree which:
///  - a leaf represents a single block
///  - a branch represents a forked
#[derive(Debug, Clone)]
pub enum ForkedBlock<A> {
	Block(A),
	Fork(Vec<ForkedBlock<A>>),
}

impl<A> ForkedBlock<A> {
	pub fn map<B>(self, f: &mut impl FnMut(A) -> B) -> ForkedBlock<B> {
		use ForkedBlock::*;
		match self {
			Block(a) => Block(f(a)),
			Fork(blocks) => Fork(blocks.into_iter().map(|block| block.map(f)).collect()),
		}
	}
}

/// Parameters describing what kind of forked blocks are generated.
#[derive(Debug, Clone)]
pub struct ConsumerParameters {
	take: Range<usize>,
	max_drop: usize,
	max_ignore: usize,

	// settings for `data delays`
	time_steps_per_block: RangeInclusive<usize>,
	max_data_delay: usize,
}

#[derive(Debug, Clone)]
pub struct ConsumerChainParameters {
	max_mainchain_length: usize,

	max_fork_count: u32,
	max_block_count: u32,
	max_fork_length: usize,

	item_parameters: ConsumerParameters,
}

pub fn generate_consumer(p: ConsumerParameters) -> impl Strategy<Value = Consumer> {
	let p = p.clone();
	(0..=p.max_drop, 0..=p.max_ignore, p.take, p.time_steps_per_block).prop_flat_map(
		move |(drop, ignore, take, time_steps)| {
			vec(0..p.max_data_delay, time_steps).prop_map(move |data_delays| Consumer {
				drop,
				ignore,
				take,
				data_delays,
			})
		},
	)
}

pub fn trim_forked_block<A>(block: ForkedBlock<A>, max_length: usize) -> ForkedBlock<A> {
	match block {
		ForkedBlock::Block(block) => ForkedBlock::Block(block),
		ForkedBlock::Fork(chain) => ForkedBlock::Fork(trim_forked_chain(chain, max_length)),
	}
}

pub fn trim_forked_chain<A>(chain: ForkedChain<A>, max_length: usize) -> ForkedChain<A> {
	chain
		.into_iter()
		.enumerate()
		.map(|(height, block)| trim_forked_block(block, max_length.saturating_sub(height)))
		.take(max_length)
		.collect()
}

pub fn generate_consumer_block(
	p: ConsumerChainParameters,
) -> impl Strategy<Value = ForkedBlock<Consumer>> {
	let leaf = generate_consumer(p.item_parameters).prop_map(ForkedBlock::Block);
	leaf.prop_recursive(
		p.max_fork_count,
		p.max_block_count,
		p.max_fork_length as u32,
		move |inner| vec(inner, 1..p.max_fork_length).prop_map(ForkedBlock::Fork),
	)
	// enforce fork length by deleting blocks that go over the limit
	.prop_map(move |block| trim_forked_block(block, p.max_fork_length))
}

pub fn generate_consumer_chain(
	p: ConsumerChainParameters,
) -> impl Strategy<Value = ForkedChain<Consumer>> {
	let single = generate_consumer(p.item_parameters.clone()).prop_map(ForkedBlock::Block);
	let forked = generate_consumer_block(p.clone());
	vec(prop_oneof![single, forked], 0..=p.max_mainchain_length)
}

/// A (possibly forked) chain with content `A` is a vector of ordered trees.
type ForkedChain<A> = Vec<ForkedBlock<A>>;

/// A data structure which describes how to construct a
/// block from a "stream" of items.
#[derive(Clone, Debug)]
pub struct Consumer {
	/// How many items to ignore from the beginning of the stream.
	/// These are not being consumed and not being removed.
	ignore: usize,

	/// How many items to drop from the stream (after ignoring `ignore`).
	drop: usize,

	/// How many items to take and include in this block (after dropping).
	take: usize,

	/// Unrelated to the (ignore/drop/take) consumer notion, but currently part
	/// of this structure. This describes how views of the chainstate are generated
	/// when this block is the "current" one.
	data_delays: Vec<usize>,
}

pub struct FilledBlock<E> {
	data: Vec<E>,
	data_delays: Vec<usize>,
}

pub fn fill_block<E: Clone>(
	input: ForkedBlock<Consumer>,
	events: &mut Vec<E>,
) -> ForkedBlock<FilledBlock<E>> {
	use ForkedBlock::*;
	match input {
		Block(Consumer { ignore, drop, take, data_delays }) => {
			{
				let _dropped_events = events.drain(ignore..(ignore + drop));
			}
			let block_events = events.drain(ignore..(ignore + take));
			Block(FilledBlock { data: block_events.collect(), data_delays: data_delays.clone() })
		},
		Fork(blocks) => Fork(fill_chain(blocks, &mut events.clone())),
	}
}

pub fn fill_chain<E: Clone>(
	chain: ForkedChain<Consumer>,
	events: &mut Vec<E>,
) -> ForkedChain<FilledBlock<E>> {
	chain.into_iter().map(|block| fill_block(block, events)).collect()
}

pub fn create_time_steps<E: Clone>(chain: &ForkedChain<FilledBlock<E>>) -> Vec<FlatChain<E>> {
	let mut chains = Vec::new();
	let mut current_blocks = Vec::new();
	use ForkedBlock::*;
	for block in chain {
		match block {
			Block(FilledBlock { data, data_delays }) => {
				current_blocks.push(FlatBlock { events: data.clone() });
				chains.push(FlatChain {
					blocks: current_blocks.clone(),
					data_delays: data_delays.clone(),
				});
			},
			Fork(forked_blocks) => {
				let forked_chains = create_time_steps(forked_blocks);
				chains.extend(forked_chains.into_iter().map(|mut forked_chain| {
					let mut extended_current_chain = current_blocks.clone();
					extended_current_chain.append(&mut forked_chain.blocks);
					FlatChain {
						blocks: extended_current_chain,
						data_delays: forked_chain.data_delays,
					}
				}));
			},
		}
	}
	chains
}

// Temp
#[derive(Debug, Clone)]
pub struct FlatBlock<E> {
	pub events: Vec<E>,
}

// Temp
#[derive(Debug, Clone)]
pub struct FlatChain<E> {
	blocks: Vec<FlatBlock<E>>,
	data_delays: Vec<usize>,
}

pub fn make_events() -> Vec<char> {
	(0..26u8)
		.into_iter()
		.map(|x| (b'a' + x) as char)
		.chain((0..26u8).into_iter().map(|x| (b'A' + x) as char))
		.chain((0..10u8).into_iter().map(|x| (b'0' + x) as char))
		.collect()
}

pub fn generate_blocks_with_tail() -> impl Strategy<Value = ForkedFilledChain> {
	let p = ConsumerChainParameters {
		max_mainchain_length: 10,
		max_fork_count: 4,
		max_block_count: 200,
		max_fork_length: 4,
		item_parameters: ConsumerParameters {
			take: 3..5,
			max_drop: 1,
			max_ignore: 2,
			time_steps_per_block: 1..=3,
			max_data_delay: 2,
		},
	};
	generate_consumer_chain(p)
		// turn into chain progression
		.prop_map(|mut blocks| {
			// generate a large number of empty blocks, so all processors can run until completion
			blocks.extend((0..8).map(|_| {
				ForkedBlock::Block(Consumer {
					ignore: 0,
					drop: 0,
					take: 0,
					data_delays: vec![0, 0, 0, 0, 0],
				})
			}));

			let filled_chain = fill_chain(blocks, &mut make_events());

			ForkedFilledChain { blocks: filled_chain }
		})
}

pub struct ForkedFilledChain {
	pub blocks: ForkedChain<FilledBlock<char>>,
}

pub fn print_blocks(
	blocks: &ForkedChain<FilledBlock<char>>,
	height: usize,
	f: &mut core::fmt::Formatter<'_>,
) -> core::fmt::Result {
	let mut forks = Vec::new();

	let mut current_string = String::from_iter((0..height).map(|_| ' '));
	current_string.push_str("|- ");

	use ForkedBlock::*;
	for block in blocks {
		match block {
			Fork(blocks) => forks.push((current_string.len(), blocks)),
			Block(FilledBlock { data, data_delays }) =>
				current_string.push_str(&format!("{data:?} [{data_delays:?}] -> ")),
		}
	}

	writeln!(f, "{current_string}")?;

	for (indent, fork) in forks {
		print_blocks(fork, indent, f)?;
	}

	Ok(())
}

impl Debug for ForkedFilledChain {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		writeln!(f, "chains:")?;
		print_blocks(&self.blocks, 0, f)
	}
}

pub fn blocks_into_chain_progression(
	filled_chain: &ForkedChain<FilledBlock<char>>,
) -> FlatChainProgression<char> {
	let time_steps = create_time_steps(&filled_chain);

	let mut chain_progression = FlatChainProgression { chains: time_steps, age: 0 };

	// attach dummy first block
	for chain in &mut chain_progression.chains {
		chain.blocks.insert(0, FlatBlock { events: vec![] });
	}

	chain_progression
}

pub struct MockChain<E, T: ChainTypes<ChainBlockHash = Vec<E>>> {
	pub chain: Vec<FlatBlock<E>>,
	pub _phantom: std::marker::PhantomData<T>,
}

use crate::electoral_systems::block_height_tracking::{primitives::Header, ChainTypes};
use sp_std::iter::Step;
// type MockChain<E> = Vec<FlatBlock<E>>;
type N = u8;

impl<E: Clone + PartialEq, T: ChainTypes<ChainBlockHash = Vec<E>>> MockChain<E, T> {
	/// NOTE: This is going to return values even for non-existent blocks!
	pub fn get_hash_by_height(&self, height: usize) -> Vec<E> {
		self.chain
			.iter()
			.take(height + 1)
			.flat_map(|block| block.events.clone())
			.collect()
	}

	pub fn get_block_height(&self) -> T::ChainBlockNumber {
		T::ChainBlockNumber::zero()
			.saturating_forward(self.chain.len())
			.saturating_backward(1)
	}
	pub fn get_block_header(&self, height: T::ChainBlockNumber) -> Option<Header<T>> {
		let height_usize: usize = T::ChainBlockNumber::steps_between(&BlockZero::zero(), &height).0;

		// let hash = self.chain.get(height_usize)?.events.clone();
		let hash = self.get_hash_by_height(height_usize);
		let parent_hash = self.get_hash_by_height(height_usize.saturating_sub(1));

		Some(Header { block_height: height, hash, parent_hash })
	}
	pub fn get_block_by_hash(&self, hash: T::ChainBlockHash) -> Option<Vec<E>> {
		self.chain
			.iter()
			.enumerate()
			.find(|(height, _)| self.get_hash_by_height(*height) == hash)
			.map(|(height, block)| block.events.clone())
	}
	pub fn get_best_block(&self) -> Header<T> {
		let hash = self.chain.last().unwrap().events.clone();
		let parent_hash = self
			.chain
			.iter()
			.rev()
			.skip(1)
			.next()
			.map(|block| block.events.clone())
			.unwrap_or(vec![]);
		Header { block_height: self.get_block_height(), hash, parent_hash }
	}
}

// Temp
#[derive(Debug, Clone)]
pub struct FlatChainProgression<E> {
	chains: Vec<FlatChain<E>>,
	age: usize,
}

impl<E: Clone> FlatChainProgression<E> {
	pub fn get_next_chain(&mut self) -> Option<Vec<FlatBlock<E>>> {
		let mut age_left = self.age;
		for (chain_count, chain) in self.chains.iter().enumerate() {
			if age_left < chain.data_delays.len() {
				self.age += 1;
				return Some(
					self.chains
						.get(chain_count.saturating_sub(*chain.data_delays.get(age_left).unwrap()))
						.unwrap()
						.blocks
						.clone(),
				);
			} else {
				age_left -= chain.data_delays.len();
			}
		}
		None
	}

	pub fn has_chains(&self) -> bool {
		self.chains.len() > 0
	}

	pub fn get_final_chain(&self) -> Vec<FlatBlock<E>> {
		self.chains.last().unwrap().blocks.clone()
	}
}
