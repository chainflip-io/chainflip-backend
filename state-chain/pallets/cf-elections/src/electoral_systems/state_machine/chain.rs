
use core::ops::Range;

use proptest::prelude::*;
use proptest::collection::*;
use proptest::test_runner::TestRunner;
use proptest::strategy::ValueTree;



#[derive(Debug, Clone)]
pub enum Block {
    Block{ jump: usize, drop: usize, take: usize },
    Fork(Vec<Block>)
}

#[derive(Debug, Clone)]
pub enum FilledBlocks<E> {
    Block {events: Vec<E>, hidden: usize},
    Fork(Vec<FilledBlocks<E>>)
}

/// First this function generates a chain,
/// which is then filled with events
pub fn generate_blocks(
    max_fork_count: u32,
    max_block_count: u32,
    max_chain_length: usize,
    block_size: Range<usize>,
    max_drop: usize,
    max_jump: usize
) -> impl Strategy<Value = Block> {

    let leaf = (0..=max_drop, 0..=max_jump, block_size).prop_map(
        |(drop, jump, take)| Block::Block { drop, jump, take }
    );
    leaf.prop_recursive(max_fork_count, max_block_count, max_chain_length as u32, move |inner| {
        vec(inner, 1..max_chain_length).prop_map(Block::Fork)
    })
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
                consumed += drop+take;
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

pub fn fill_blocks<E: Clone>(block: &Block, mut events: Vec<E>) -> (FilledBlocks<E>, Vec<E>) {
    match block {
        Block::Block { jump, drop, take } => {
            let block_events = events.drain(jump..&(jump+drop+take));
            let block = FilledBlocks::Block { events: block_events.collect(), hidden: *drop };
            (block, events)
        },
        Block::Fork(blocks) => {
            let mut result = Vec::new();
            let mut fork_events = events.clone();
            for block in blocks {
                let (filled, res_fork_events) = fill_blocks(block, fork_events);
                fork_events = res_fork_events;
                result.push(filled);
            }
            (FilledBlocks::Fork(result), events)
        },
    }
}


#[test]
pub fn test_test() {

    let mut runner = TestRunner::default();
    let filled_chain = generate_blocks(5, 100, 4, 3..5, 1, 2)
        .prop_flat_map(|block| {
            let (cursor, consumed) = block.size();
            let char = (0..26u8).prop_map(|x| 
                (b'a' + x) as char
            );
            vec(char, consumed + cursor)
                .prop_map(move |events| {
                    println!("input events: {events:?}");
                    fill_blocks(&block, events).0
            })
        })
        .new_tree(&mut runner).unwrap().current();

    // let (cursor, consumed) = chain.size();
    // println!("cursor: {cursor}, consumed: {consumed}, chain: {chain:?}");
    println!("chain: {filled_chain:?}");
    assert!(false);
}
