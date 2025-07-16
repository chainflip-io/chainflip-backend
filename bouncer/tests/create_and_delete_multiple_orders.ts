import assert from 'assert';
import { createStateChainKeypair, lpMutex, waitForExt } from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { limitOrder } from 'shared/limit_order';
import { rangeOrder } from 'shared/range_order';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { deposits } from 'shared/setup_swaps';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';

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

export async function createAndDeleteMultipleOrders(
  logger: Logger,
  numberOfLimitOrders = 30,
  lpKey?: string,
) {
  await using chainflip = await getChainflipApi();

  const lpUri = lpKey || DEFAULT_LP;
  const lp = createStateChainKeypair(lpUri);

  logger.info(`Depositing liquidity to ${lpUri}`);

  await Promise.all([
    // provide liquidity to LP_3
    depositLiquidity(logger, 'Usdc', 10000, false, lpUri),
    depositLiquidity(logger, 'Eth', deposits.get('Eth')!, false, lpUri),
    depositLiquidity(logger, 'Dot', deposits.get('Dot')!, false, lpUri),
    depositLiquidity(logger, 'Btc', deposits.get('Btc')!, false, lpUri),
    depositLiquidity(logger, 'Flip', deposits.get('Flip')!, false, lpUri),
    depositLiquidity(logger, 'Usdt', deposits.get('Usdt')!, false, lpUri),
    depositLiquidity(logger, 'ArbEth', deposits.get('ArbEth')!, false, lpUri),
    depositLiquidity(logger, 'ArbUsdc', deposits.get('ArbUsdc')!, false, lpUri),
    depositLiquidity(logger, 'Sol', deposits.get('Sol')!, false, lpUri),
    depositLiquidity(logger, 'SolUsdc', deposits.get('SolUsdc')!, false, lpUri),
  ]);

  logger.info(`Liquidity successfully deposited to ${lpUri}`);

  // create a series of limit_order and save their info to delete them later on
  const promises = [];
  const ordersToDelete: {
    Limit?: { base_asset: string; quote_asset: string; side: string; id: number };
    Range?: { base_asset: string; quote_asset: string; id: number };
  }[] = [];

  for (let i = 1; i <= numberOfLimitOrders; i++) {
    promises.push(limitOrder(logger, 'Btc', 0.00000001, i, i, lpUri));
    ordersToDelete.push({ Limit: { base_asset: 'BTC', quote_asset: 'USDC', side: 'sell', id: i } });
  }
  for (let i = 1; i <= numberOfLimitOrders; i++) {
    promises.push(limitOrder(logger, 'Eth', 0.000000000000000001, i, i, lpUri));
    ordersToDelete.push({ Limit: { base_asset: 'ETH', quote_asset: 'USDC', side: 'sell', id: i } });
  }

  promises.push(rangeOrder(logger, 'Btc', 0.1, lpUri, 0));
  ordersToDelete.push({
    Range: { base_asset: 'BTC', quote_asset: 'USDC', id: 0 },
  });
  promises.push(rangeOrder(logger, 'Eth', 0.01, lpUri, 0));
  ordersToDelete.push({
    Range: { base_asset: 'ETH', quote_asset: 'USDC', id: 0 },
  });

  logger.info('Submitting orders');
  await Promise.all(promises);
  logger.info('Orders successfully submitted');

  let openOrders = await countOpenOrders('BTC', 'USDC', lp.address);
  openOrders += await countOpenOrders('ETH', 'USDC', lp.address);
  logger.info(`Number of open orders: ${openOrders}`);

  logger.info('Deleting opened orders...');

  const release = await lpMutex.acquire();
  const { promise, waiter } = waitForExt(chainflip, logger, 'InBlock', release);
  const nonce = (await chainflip.rpc.system.accountNextIndex(lp.address)) as unknown as number;
  await chainflip.tx.liquidityPools
    .cancelOrdersBatch(ordersToDelete)
    .signAndSend(lp, { nonce }, waiter);

  await promise;

  logger.info('All orders successfully deleted');

  openOrders = await countOpenOrders('BTC', 'USDC', lp.address);
  openOrders += await countOpenOrders('ETH', 'USDC', lp.address);
  logger.info(`Number of open orders: ${openOrders}`);

  assert.strictEqual(openOrders, 0, `Number of open orders should be 0 but is ${openOrders}`);

  logger.info('All orders successfully deleted');
}

export async function testCancelOrdersBatch(testContext: TestContext) {
  await createAndDeleteMultipleOrders(testContext.logger);
}
