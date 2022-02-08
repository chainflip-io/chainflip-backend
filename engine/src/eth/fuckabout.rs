use std::{pin::Pin, vec::IntoIter};

use futures::{
    stream::{self, select, Iter},
    Stream,
};

use futures::StreamExt;

pub struct Block1 {
    number: u64,
}

pub struct Block2 {
    number: u64,
}

pub trait Blockable {
    fn number(&self) -> u64;
}

impl Blockable for Block1 {
    fn number(&self) -> u64 {
        self.number
    }
}

impl Blockable for Block2 {
    fn number(&self) -> u64 {
        self.number
    }
}

pub enum BlockEnum {
    Block1(Block1),
    Block2(Block2),
}

impl Blockable for BlockEnum {
    fn number(&self) -> u64 {
        match self {
            BlockEnum::Block1(block1) => block1.number(),
            BlockEnum::Block2(block2) => block2.number(),
        }
    }
}

pub async fn my_merged_stream<BlockHeaderableStream>(
    stream1: BlockHeaderableStream,
    stream2: BlockHeaderableStream,
    // stream2: BlockHeaderableStream2,
) -> impl Stream<Item = u64>
where
    BlockHeaderableStream: Stream<Item = BlockEnum> + 'static,
    // BlockHeaderableStream2: Stream<Item = BlockEnum> + 'static,
{
    let merged_stream = select(stream1, stream2);
    merged_stream.map(|block| block.number())
}

pub async fn stream1() -> Iter<IntoIter<BlockEnum>> {
    // let block1: Box<dyn Blockable> = Box::new(Block1 { number: 1 });
    let block1 = BlockEnum::Block1(Block1 { number: 1 });
    let stream1 = stream::iter(vec![block1]);
    stream1
}

pub async fn stream2() -> Iter<IntoIter<BlockEnum>> {
    // let block1: Box<dyn Blockable> = Box::new(Block1 { number: 1 });
    let block2 = BlockEnum::Block2(Block2 { number: 1 });
    let stream2 = stream::iter(vec![block2]);
    stream2
}

#[tokio::test]
async fn my_stuff() {
    let block2: Box<dyn Blockable> = Box::new(Block2 { number: 1 });

    let stream1 = stream1().await;

    let stream2 = stream2().await;

    let my_stream = my_merged_stream(stream1, stream2).await;
}
