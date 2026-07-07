import assert from 'assert';
import { getChainflipApi } from 'shared/utils/substrate';
import type { PalletCfPoolsCloseOrder } from 'generated/chaintypes/chainflip-node';
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
  const orders = await chainflip.rpc.cf_pool_orders(baseAsset, quoteAsset, lp);
  if (!orders) {
    throw Error('Rpc cf_pool_orders returned undefined');
  }
  let openOrders = 0;

  openOrders += orders?.limit_orders.asks.length || 0;
  openOrders += orders?.limit_orders.bids.length || 0;
  openOrders += orders?.range_orders.length || 0;

  return openOrders;
}

export async function createAndDeleteMultipleOrders<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  numberOfLimitOrders = 30,
) {
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
  const promises: ((api: ChainflipIO<A>) => Promise<void>)[] = [];
  const ordersToDelete: PalletCfPoolsCloseOrder[] = [];

  for (let i = 1; i <= numberOfLimitOrders; i++) {
    promises.push((subcf) => limitOrder(subcf, 'Btc', 0.0001, i, i));
    ordersToDelete.push({
      type: 'Limit',
      value: { baseAsset: 'Btc', quoteAsset: 'Usdc', side: 'Sell', id: BigInt(i) },
    });
  }
  for (let i = 1; i <= numberOfLimitOrders; i++) {
    promises.push((subcf) => limitOrder(subcf, 'Eth', 0.003, i, i));
    ordersToDelete.push({
      type: 'Limit',
      value: { baseAsset: 'Eth', quoteAsset: 'Usdc', side: 'Sell', id: BigInt(i) },
    });
  }

  promises.push((subcf) => rangeOrder(subcf, 'Btc', 0.1, 0));
  ordersToDelete.push({
    type: 'Range',
    value: { baseAsset: 'Btc', quoteAsset: 'Usdc', id: 0n },
  });
  promises.push((subcf) => rangeOrder(subcf, 'Eth', 0.01, 0));
  ordersToDelete.push({
    type: 'Range',
    value: { baseAsset: 'Eth', quoteAsset: 'Usdc', id: 0n },
  });

  cf.debug('Submitting orders');
  await cf.all(promises);
  cf.debug('Orders successfully submitted');

  let openOrders = await countOpenOrders('BTC', 'USDC', lpAddress);
  openOrders += await countOpenOrders('ETH', 'USDC', lpAddress);
  cf.debug(`Number of open orders: ${openOrders}`);

  cf.debug('Deleting opened orders...');
  await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.liquidityPools.cancelOrdersBatch(ordersToDelete),
  });
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
