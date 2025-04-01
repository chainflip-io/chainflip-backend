use core::ops::Range;

use proptest::{collection::*, prelude::*, strategy::ValueTree, test_runner::TestRunner};

#[derive(Debug, Clone)]
pub enum Block {
	Block { jump: usize, drop: usize, take: usize },
	Fork(Vec<Block>),
}

#[derive(Debug, Clone)]
pub enum FilledBlocks<E> {
	Block { events: Vec<E> },
	Fork(Vec<FilledBlocks<E>>),
}

#[derive(Debug, Clone)]
pub struct FlatBlock<E> {
	events: Vec<E>,
}

#[derive(Debug, Clone)]
pub struct FlatChain<E> {
	blocks: Vec<FlatBlock<E>>,
}

/// First this function generates a chain,
/// which is then filled with events
pub fn generate_block(
	max_fork_count: u32,
	max_block_count: u32,
	max_fork_length: usize,
	block_size: Range<usize>,
	max_drop: usize,
	max_jump: usize,
) -> impl Strategy<Value = Block> {
	let leaf = (0..=max_drop, 0..=max_jump, block_size)
		.prop_map(|(drop, jump, take)| Block::Block { drop, jump, take });
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
	max_drop: usize,
	max_jump: usize,
) -> impl Strategy<Value = Vec<Block>> {
	vec(
		generate_block(
			max_fork_count,
			max_block_count,
			max_fork_length,
			block_size,
			max_drop,
			max_jump,
		),
		0..=max_mainchain_length,
	)
}

// output (max_cursor, consumed)
pub fn size(blocks: &Vec<Block>) -> (usize, usize) {
	let mut cursor = 0;
	let mut consumed = 0;

	let mut consumed_forks = Vec::new();

	for block in blocks {
		match block {
			Block::Block { jump, drop, take } => {
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
			Block::Block { jump, drop, take } => (*jump, drop + take),
			Block::Fork(blocks) => size(blocks),
		}
	}
}

pub fn fill_block<E: Clone>(block: &Block, mut events: Vec<E>) -> (FilledBlocks<E>, Vec<E>) {
	match block {
		Block::Block { jump, drop, take } => {
			{
				let _dropped_events = events.drain(jump..&(jump + drop));
			}
			let block_events = events.drain(jump..&(jump + take));
			let block = FilledBlocks::Block { events: block_events.collect() };
			(block, events)
		},
		Block::Fork(blocks) => {
			// let mut result = Vec::new();
			// let mut fork_events = events.clone();
			// for block in blocks {
			//     let (filled, res_fork_events) = fill_block(block, fork_events);
			//     fork_events = res_fork_events;
			//     result.push(filled);
			// }
			// (FilledBlocks::Fork(result), events)
			(FilledBlocks::Fork(fill_blocks(blocks, events.clone())), events)
		},
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
	let mut current_chain = FlatChain { blocks: Vec::new() };
	for block in blocks {
		match block {
			FilledBlocks::Block { events } => {
				current_chain.blocks.push(FlatBlock { events: events.clone() });
				chains.push(current_chain.clone());
			},
			FilledBlocks::Fork(forked_blocks) => {
				let forked_chains = create_time_steps(forked_blocks);
				chains.extend(forked_chains.into_iter().map(|mut forked_chain| {
					let mut extended_current_chain = current_chain.clone();
					extended_current_chain.blocks.append(&mut forked_chain.blocks);
					extended_current_chain
				}));
			},
		}
	}
	chains
}

pub fn make_events(len: usize) -> Vec<char> {
	(0..26u8).into_iter().map(|x| (b'a' + x) as char).take(len).collect()
}

#[test]
pub fn test_test() {
	let mut runner = TestRunner::default();
	let filled_chain = generate_blocks(10, 4, 50, 4, 3..5, 1, 2)
		.prop_map(|block| {
			let (cursor, consumed) = size(&block);
			fill_blocks(&block, make_events(consumed + cursor))
		})
		.new_tree(&mut runner)
		.unwrap()
		.current();

	println!("chain: {filled_chain:?}");

	let time_steps = create_time_steps(&filled_chain);

	for step in time_steps {
		println!("step: {step:?}");
	}
	assert!(false);
}
