import assert from 'assert';
import { cfMutex, waitForExt } from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { limitOrder } from 'shared/limit_order';
import { rangeOrder } from 'shared/range_order';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { deposits } from 'shared/setup_swaps';
import { TestContext } from 'shared/utils/test_context';
import {
  ChainflipIO,
  fullAccountFromUri,
  newChainflipIO,
  WithLpAccount,
} from 'shared/utils/chainflip_io';

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

export async function createAndDeleteMultipleOrders<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  numberOfLimitOrders = 30,
) {
  await using chainflip = await getChainflipApi();
  const lpUri = cf.requirements.account.uri;
  const lpAddress = cf.requirements.account.keypair.address;

  cf.debug(`Depositing liquidity to ${lpUri}`);

  await cf.all([
    // provide liquidity to lpUri
    (subcf) => depositLiquidity(subcf, 'Usdc', 10000),
    (subcf) => depositLiquidity(subcf, 'Eth', deposits.get('Eth')!),
    (subcf) => depositLiquidity(subcf, 'HubDot', deposits.get('HubDot')!),
    (subcf) => depositLiquidity(subcf, 'Btc', deposits.get('Btc')!),
    (subcf) => depositLiquidity(subcf, 'Flip', deposits.get('Flip')!),
    (subcf) => depositLiquidity(subcf, 'Usdt', deposits.get('Usdt')!),
    (subcf) => depositLiquidity(subcf, 'ArbEth', deposits.get('ArbEth')!),
    (subcf) => depositLiquidity(subcf, 'ArbUsdc', deposits.get('ArbUsdc')!),
    (subcf) => depositLiquidity(subcf, 'Sol', deposits.get('Sol')!),
    (subcf) => depositLiquidity(subcf, 'SolUsdc', deposits.get('SolUsdc')!),
  ]);

  cf.debug(`Liquidity successfully deposited to ${lpUri}`);

  // create a series of limit_order and save their info to delete them later on
  const promises = [];
  const ordersToDelete: {
    Limit?: { base_asset: string; quote_asset: string; side: string; id: number };
    Range?: { base_asset: string; quote_asset: string; id: number };
  }[] = [];

  for (let i = 1; i <= numberOfLimitOrders; i++) {
    promises.push(limitOrder(cf.logger, 'Btc', 0.00000001, i, i, lpUri));
    ordersToDelete.push({ Limit: { base_asset: 'BTC', quote_asset: 'USDC', side: 'sell', id: i } });
  }
  for (let i = 1; i <= numberOfLimitOrders; i++) {
    promises.push(limitOrder(cf.logger, 'Eth', 0.000000000000000001, i, i, lpUri));
    ordersToDelete.push({ Limit: { base_asset: 'ETH', quote_asset: 'USDC', side: 'sell', id: i } });
  }

  promises.push(rangeOrder(cf.logger, 'Btc', 0.1, lpUri, 0));
  ordersToDelete.push({
    Range: { base_asset: 'BTC', quote_asset: 'USDC', id: 0 },
  });
  promises.push(rangeOrder(cf.logger, 'Eth', 0.01, lpUri, 0));
  ordersToDelete.push({
    Range: { base_asset: 'ETH', quote_asset: 'USDC', id: 0 },
  });

  cf.debug('Submitting orders');
  await Promise.all(promises);
  cf.debug('Orders successfully submitted');

  let openOrders = await countOpenOrders('BTC', 'USDC', lpAddress);
  openOrders += await countOpenOrders('ETH', 'USDC', lpAddress);
  cf.debug(`Number of open orders: ${openOrders}`);

  cf.debug('Deleting opened orders...');

  const release = await cfMutex.acquire(lpUri);
  const { promise, waiter } = waitForExt(chainflip, cf.logger, 'InBlock', release);
  const nonce = (await chainflip.rpc.system.accountNextIndex(lpAddress)) as unknown as number;
  await chainflip.tx.liquidityPools
    .cancelOrdersBatch(ordersToDelete)
    .signAndSend(cf.requirements.account.keypair, { nonce }, waiter);

  await promise;

  cf.debug('All orders successfully deleted');

  openOrders = await countOpenOrders('BTC', 'USDC', lpAddress);
  openOrders += await countOpenOrders('ETH', 'USDC', lpAddress);
  cf.debug(`Number of open orders: ${openOrders}`);

  assert.strictEqual(openOrders, 0, `Number of open orders should be 0 but is ${openOrders}`);

  cf.debug('All orders successfully deleted');
}

export async function testCancelOrdersBatch(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, {
    account: fullAccountFromUri('//LP_3', 'LP'),
  });
  await createAndDeleteMultipleOrders(cf);
}
