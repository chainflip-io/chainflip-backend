use core::{fmt::Debug, ops::{Range, RangeInclusive}};
use proptest::{collection::*, prelude::*};

use crate::electoral_systems::block_height_tracking::primitives::Header;

#[derive(Debug, Clone)]
pub enum Block {
	Block { jump: usize, drop: usize, take: usize, time_steps: usize, data_delays: Vec<usize> },
	Fork(Vec<Block>),
}

#[derive(Debug, Clone)]
pub enum FilledBlocks<E> {
	Block { events: Vec<E>, time_steps: usize, data_delays: Vec<usize> },
	Fork(Vec<FilledBlocks<E>>),
}

#[derive(Debug, Clone)]
pub struct FlatBlock<E> {
	pub events: Vec<E>,
}

#[derive(Debug, Clone)]
pub struct FlatChain<E> {
	blocks: Vec<FlatBlock<E>>,
	data_delays: Vec<usize>,
}

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

pub fn generate_leaf_block(
	block_size: Range<usize>,
	time_steps_per_block: RangeInclusive<usize>,
	max_data_delay: usize,
	max_drop: usize,
	max_jump: usize,
) -> impl Strategy<Value = Block> {
	(0..=max_drop, 0..=max_jump, block_size, time_steps_per_block).prop_flat_map(
		move |(drop, jump, take, time_steps)| {
			vec(0..max_data_delay, time_steps).prop_map(move |data_delays| Block::Block {
				drop,
				jump,
				take,
				time_steps,
				data_delays,
			})
		},
	)
}

/// First this function generates a chain,
/// which is then filled with events
pub fn generate_block(
	max_fork_count: u32,
	max_block_count: u32,
	max_fork_length: usize,
	block_size: Range<usize>,
	time_steps_per_block: RangeInclusive<usize>,
	max_data_delay: usize,
	max_drop: usize,
	max_jump: usize,
) -> impl Strategy<Value = Block> {
	let leaf =
		generate_leaf_block(block_size, time_steps_per_block, max_data_delay, max_drop, max_jump);
	leaf.prop_recursive(max_fork_count, max_block_count, max_fork_length as u32, move |inner| {
		vec(inner, 1..max_fork_length).prop_map(Block::Fork)
	})
}

pub fn generate_blocks(
	max_mainchain_length: usize,
	max_fork_count: u32,
	max_block_count: u32,
	max_fork_length: usize,
	block_size: Range<usize>,
	time_steps_per_block: RangeInclusive<usize>,
	max_data_delay: usize,
	max_drop: usize,
	max_jump: usize,
) -> impl Strategy<Value = Vec<Block>> {
	let single = generate_block(
		max_fork_count,
		max_block_count,
		max_fork_length,
		block_size.clone(),
		time_steps_per_block.clone(),
		max_data_delay,
		max_drop,
		max_jump,
	);
	let forked =
		generate_leaf_block(block_size, time_steps_per_block, max_data_delay, max_drop, max_jump);
	vec(prop_oneof![single, forked], 0..=max_mainchain_length)
}

// output (max_cursor, consumed)
pub fn size(blocks: &Vec<Block>) -> (usize, usize) {
	let mut cursor = 0;
	let mut consumed = 0;

	let mut consumed_forks = Vec::new();

	for block in blocks {
		match block {
			Block::Block { jump, drop, take, time_steps: _, data_delays: _ } => {
				cursor = std::cmp::max(cursor, *jump);
				consumed += drop + take;
			},
			Block::Fork(blocks) => {
				let (cursor_fork, consumed_fork) = size(blocks);
				cursor = std::cmp::max(cursor, cursor_fork);
				consumed_forks.push(consumed_fork + consumed);
			},
		}
	}
	(cursor, std::cmp::max(consumed, *consumed_forks.iter().max().unwrap_or(&0)))
}

impl Block {
	pub fn size(&self) -> (usize, usize) {
		match self {
			Block::Block { jump, drop, take, time_steps: _, data_delays: _ } =>
				(*jump, drop + take),
			Block::Fork(blocks) => size(blocks),
		}
	}
}

pub fn fill_block<E: Clone>(block: &Block, mut events: Vec<E>) -> (FilledBlocks<E>, Vec<E>) {
	match block {
		Block::Block { jump, drop, take, time_steps, data_delays } => {
			{
				let _dropped_events = events.drain(jump..&(jump + drop));
			}
			let block_events = events.drain(jump..&(jump + take));
			let block = FilledBlocks::Block {
				events: block_events.collect(),
				time_steps: *time_steps,
				data_delays: data_delays.clone(),
			};
			(block, events)
		},
		Block::Fork(blocks) => (FilledBlocks::Fork(fill_blocks(blocks, events.clone())), events),
	}
}

pub fn fill_blocks<E: Clone>(blocks: &Vec<Block>, mut events: Vec<E>) -> Vec<FilledBlocks<E>> {
	let mut result = Vec::new();
	for block in blocks {
		let (filled, res_fork_events) = fill_block(block, events);
		events = res_fork_events;
		result.push(filled);
	}
	result
}

pub fn create_time_steps<E: Clone>(blocks: &Vec<FilledBlocks<E>>) -> Vec<FlatChain<E>> {
	let mut chains = Vec::new();
	let mut current_blocks = Vec::new();
	for block in blocks {
		match block {
			FilledBlocks::Block { events, time_steps: _, data_delays } => {
				current_blocks.push(FlatBlock { events: events.clone() });
				chains.push(FlatChain {
					blocks: current_blocks.clone(),
					data_delays: data_delays.clone(),
				});
			},
			FilledBlocks::Fork(forked_blocks) => {
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

pub fn make_events(len: usize) -> Vec<char> {
	(0..26u8)
		.into_iter()
		.map(|x| (b'a' + x) as char)
		.chain((0..26u8).into_iter().map(|x| (b'A' + x) as char))
		.chain((0..10u8).into_iter().map(|x| (b'0' + x) as char))
		.take(len)
		.collect()
}

pub fn generate_blocks_with_tail() -> impl Strategy<Value = BlocksWithTail> {
	generate_blocks(10, 4, 200, 4, 3..5, 1..=3, 2, 1, 2)

		// turn into chain progression
		.prop_map(|mut blocks| {

			// generate a large number of empty blocks, so all processors can run until completion
			blocks.extend((0..5).map(|_| Block::Block { jump: 0, drop: 0, take: 0, time_steps: 1, data_delays: vec![0,0,0,0,0] }));

			let (cursor, consumed) = size(&blocks);
			println!("size: ({cursor:?}, {consumed:?})");
			let filled_chain = fill_blocks(&blocks, make_events(consumed + cursor));

			BlocksWithTail {
				blocks: filled_chain
			}

		})
}

pub struct BlocksWithTail {
	pub blocks: Vec<FilledBlocks<char>>
}

pub fn print_blocks(blocks: &Vec<FilledBlocks<char>>, height: usize, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
	let mut forks = Vec::new();

	let mut current_string = String::from_iter((0..height).map(|_| ' '));
	current_string.push_str("|- ");

	for block in blocks {
		match block {
			FilledBlocks::Fork(blocks) => forks.push((current_string.len(), blocks)),
			FilledBlocks::Block { events, time_steps, data_delays } => current_string.push_str(&format!("{events:?} [{data_delays:?}] -> ")),
		}
	}

	writeln!(f, "{current_string}")?;

	for (indent, fork) in forks {
		print_blocks(fork, indent, f)?;
	}

	Ok(())
}

impl Debug for BlocksWithTail {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		writeln!(f, "chains:")?;
		print_blocks(&self.blocks, 0, f)
	}
}

pub fn blocks_into_chain_progression(filled_chain: &Vec<FilledBlocks<char>>) -> FlatChainProgression<char> {
	let time_steps = create_time_steps(&filled_chain);

	for step in &time_steps {
		println!("step: {step:?}");
	}

	let mut chain_progression = FlatChainProgression { chains: time_steps, age: 0 };

	// attach dummy first block
	for chain in &mut chain_progression.chains {
		chain.blocks.insert(0, FlatBlock { events: vec![] });
	}

	chain_progression

}

type MockChain<E> = Vec<FlatBlock<E>>;
type N = u8;
pub fn get_block_height<E>(chain: &MockChain<E>) -> N {
	chain.len() as u8 - 1
}
pub fn get_block_header<E: Clone>(chain: &MockChain<E>, height: N) -> Option<Header<Vec<E>, N>> {
	let hash = chain.get(height as usize)?.events.clone();
	let parent_hash = chain
		.get(height.saturating_sub(1) as usize)
		.map(|block| block.events.clone())
		.unwrap_or(vec![]);
	Some(Header { block_height: height, hash, parent_hash })
}
pub fn get_best_block<E: Clone>(chain: &MockChain<E>) -> Header<Vec<E>, N> {
	let hash = chain.last().unwrap().events.clone();
	let parent_hash = chain
		.iter()
		.rev()
		.skip(1)
		.next()
		.map(|block| block.events.clone())
		.unwrap_or(vec![]);
	Header { block_height: chain.len() as u8 - 1, hash, parent_hash }
}
