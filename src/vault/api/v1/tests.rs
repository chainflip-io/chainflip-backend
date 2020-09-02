use super::*;

use crate::side_chain::FakeSideChain;

/// Populate the chain with 2 blocks, request all 2
#[tokio::test]
async fn get_all_two_blocks() {
    let params = BlocksQueryParams::new(0, 2);

    let mut side_chain = FakeSideChain::new();

    side_chain.add_block(vec![]).unwrap();
    side_chain.add_block(vec![]).unwrap();

    let side_chain = Arc::new(Mutex::new(side_chain));

    let res = get_blocks(params, side_chain)
        .await
        .expect("Expected success");

    assert_eq!(res.blocks.len(), 2);
    assert_eq!(res.total_blocks, 2);
}

#[tokio::test]
async fn get_two_blocks_out_of_three() {
    use crate::utils::test_utils;

    let params = BlocksQueryParams::new(0, 2);

    let mut side_chain = FakeSideChain::new();

    side_chain.add_block(vec![]).unwrap();

    let tx = test_utils::create_fake_quote_tx();

    side_chain.add_block(vec![tx.clone().into()]).unwrap();
    side_chain.add_block(vec![]).unwrap();

    let side_chain = Arc::new(Mutex::new(side_chain));

    let res = get_blocks(params, side_chain)
        .await
        .expect("Expected success");

    assert_eq!(res.blocks.len(), 2);
    assert_eq!(res.blocks[1].transactions.len(), 1);
    assert_eq!(res.total_blocks, 3);
}

#[tokio::test]
async fn cap_too_big_limit() {
    let params = BlocksQueryParams::new(1, 100);

    let mut side_chain = FakeSideChain::new();

    side_chain.add_block(vec![]).unwrap();
    side_chain.add_block(vec![]).unwrap();

    let side_chain = Arc::new(Mutex::new(side_chain));

    let res = get_blocks(params, side_chain)
        .await
        .expect("Expected success");

    assert_eq!(res.blocks.len(), 1);
    assert_eq!(res.total_blocks, 2);
}

#[tokio::test]
async fn zero_limit() {
    let params = BlocksQueryParams::new(1, 0);
    let mut side_chain = FakeSideChain::new();

    side_chain.add_block(vec![]).unwrap();
    side_chain.add_block(vec![]).unwrap();

    let side_chain = Arc::new(Mutex::new(side_chain));

    let res = get_blocks(params, side_chain)
        .await
        .expect("Expected success");

    assert_eq!(res.blocks.len(), 0);
    assert_eq!(res.total_blocks, 2);
}

#[tokio::test]
async fn blocks_do_not_exist() {
    let params = BlocksQueryParams::new(100, 2);

    let mut side_chain = FakeSideChain::new();

    side_chain.add_block(vec![]).unwrap();
    side_chain.add_block(vec![]).unwrap();

    let side_chain = Arc::new(Mutex::new(side_chain));

    let res = get_blocks(params, side_chain)
        .await
        .expect("Expected success");

    assert_eq!(res.blocks.len(), 0);
    assert_eq!(res.total_blocks, 2);
}
