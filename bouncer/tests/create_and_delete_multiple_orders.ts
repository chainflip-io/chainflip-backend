import assert from 'assert';
import { createStateChainKeypair, handleSubstrateError, lpMutex, WhaleKeyManager } from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { limitOrder } from '../shared/limit_order';
import { rangeOrder } from '../shared/range_order';
import { depositLiquidity } from '../shared/deposit_liquidity';
import { deposits } from '../shared/setup_swaps';
import { ExecutableTest } from '../shared/executable_test';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testCancelOrdersBatch = new ExecutableTest(
  'Cancel-Orders-Batch',
  createAndDeleteMultipleOrders,
  340,
);

const DEFAULT_LP: string = '//LP_3';

async function countOpenOrders(baseAsset: string, quoteAsset: string, lp: string) {
  await using chainflip = await getChainflipApi();
  const orders = await chainflip.rpc('cf_pool_orders', baseAsset, quoteAsset, lp);
  if (!orders) {
    throw Error('Rpc cf_pool_orders returned undefined');
  }
  let openOrders = 0;

  // @ts-expect-error limit_orders does not exist on type AnyJson
  openOrders += orders?.limit_orders.asks.length || 0;
  // @ts-expect-error limit_orders does not exist on type AnyJson
  openOrders += orders?.limit_orders.bids.length || 0;
  // @ts-expect-error range_orders does not exist on type AnyJson
  openOrders += orders?.range_orders.length || 0;

  return openOrders;
}

export async function createAndDeleteMultipleOrders(numberOfLimitOrders = 30, lpKey?: string) {
  await using chainflip = await getChainflipApi();

  const lpUri = lpKey || DEFAULT_LP;
  const lp = createStateChainKeypair(lpUri);

  // TODO: Look at these amounts, might be too much - why are we using the same amounts as setup swaps? doesn't make sense
  const privateKey = await WhaleKeyManager.getNextKey();
  await Promise.all([
    // provide liquidity to LP_3
    depositLiquidity('Usdc', 10000, false, lpUri, privateKey),
    depositLiquidity('Eth', 2, false, lpUri, privateKey),
    depositLiquidity('Btc', 2, false, lpUri, privateKey),
  ]);

  // create a series of limit_order and save their info to delete them later on
  const promises = [];
  const orderToDelete: {
    Limit?: { base_asset: string; quote_asset: string; side: string; id: number };
    Range?: { base_asset: string; quote_asset: string; id: number };
  }[] = [];

  for (let i = 0; i < numberOfLimitOrders; i++) {
    promises.push(limitOrder('Btc', 0.00000001, i, i, lpUri));
    orderToDelete.push({ Limit: { base_asset: 'BTC', quote_asset: 'USDC', side: 'sell', id: i } });
  }
  for (let i = 0; i < numberOfLimitOrders; i++) {
    promises.push(limitOrder('Eth', 0.000000000000000001, i, i, lpUri));
    orderToDelete.push({ Limit: { base_asset: 'ETH', quote_asset: 'USDC', side: 'sell', id: i } });
  }

  promises.push(rangeOrder('Btc', 0.1, lpUri, 0));
  orderToDelete.push({
    Range: { base_asset: 'BTC', quote_asset: 'USDC', id: 0 },
  });
  promises.push(rangeOrder('Eth', 0.01, lpUri, 0));
  orderToDelete.push({
    Range: { base_asset: 'ETH', quote_asset: 'USDC', id: 0 },
  });

  testCancelOrdersBatch.log('Submitting orders');
  await Promise.all(promises);
  testCancelOrdersBatch.log('Orders successfully submitted');

  let openOrders = await countOpenOrders('BTC', 'USDC', lp.address);
  openOrders += await countOpenOrders('ETH', 'USDC', lp.address);
  testCancelOrdersBatch.log(`Number of open orders: ${openOrders}`);

  testCancelOrdersBatch.log('Deleting opened orders...');
  const orderDeleteEvent = observeEvent('liquidityPools:RangeOrderUpdated', {
    test: (event) => event.data.lp === lp.address && event.data.baseAsset === 'Btc',
  }).event;
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .cancelOrdersBatch(orderToDelete)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await orderDeleteEvent;
  testCancelOrdersBatch.log('All orders successfully deleted');

  openOrders = await countOpenOrders('BTC', 'USDC', lp.address);
  openOrders += await countOpenOrders('ETH', 'USDC', lp.address);
  testCancelOrdersBatch.log(`Number of open orders: ${openOrders}`);

  assert.strictEqual(openOrders, 0, 'Number of open orders should be 0');
}
