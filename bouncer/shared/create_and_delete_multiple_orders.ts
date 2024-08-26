import { ApiPromise } from '@polkadot/api';
import { Keyring } from '../polkadot/keyring';
import { handleSubstrateError, lpMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { limitOrder } from './limit_order';
import { rangeOrder } from './range_order';

async function countOpenOrders(
  baseAsset: string,
  quoteAsset: string,
  lp: string,
  chainflip: ApiPromise,
) {
  const orders = await chainflip.rpc('cf_pool_orders', baseAsset, quoteAsset, lp);
  if (!orders) {
    throw Error('Rpc cf_pool_orders returned undefined');
  }
  let openOrders = 0;

  // @ts-expect-error limit_orders does not exist on type AnyJson
  openOrders += orders?.limit_orders.asks.length || 0;
  // @ts-expect-error limit_orders does not exist on type AnyJson
  openOrders += orders?.limit_orders.bids.length || 0;
  // @ts-expect-error limit_orders does not exist on type AnyJson
  openOrders += orders?.range_orders.length || 0;

  return openOrders;
}

export async function createAndDeleteMultipleOrders(numberOfOrders: number) {
  console.log(`=== cancel_orders_batch test ===`);
  await using chainflip = await getChainflipApi();

  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const lpUri = process.env.LP_URI || '//LP_3';
  const lp = keyring.createFromUri(lpUri);

  // create a series of limit_order and save their info to delete them later on
  const promises = [];
  const orderToDelete: {
    Limit?: { base_asset: string; quote_asset: string; side: string; id: number };
    Range?: { base_asset: string; quote_asset: string; id: number };
  }[] = [];
  let i = 0;
  while (i < numberOfOrders) {
    promises.push(limitOrder('Btc', 0.00000001, i, i, '//LP_3'));
    orderToDelete.push({ Limit: { base_asset: 'BTC', quote_asset: 'USDC', side: 'sell', id: i } });
    i++;
  }
  i = 0;
  while (i < numberOfOrders) {
    promises.push(limitOrder('Eth', 0.000000000000000001, i, i, '//LP_3'));
    orderToDelete.push({ Limit: { base_asset: 'ETH', quote_asset: 'USDC', side: 'sell', id: i } });
    i++;
  }
  i = 0;

  promises.push(rangeOrder('Btc', 0.1, '//LP_3'));
  orderToDelete.push({
    Range: { base_asset: 'BTC', quote_asset: 'USDC', id: 0 },
  });
  promises.push(rangeOrder('Eth', 0.01, '//LP_3'));
  orderToDelete.push({
    Range: { base_asset: 'ETH', quote_asset: 'USDC', id: 0 },
  });

  console.log('Submitting orders');
  await Promise.all(promises);
  console.log('Orders successfully submitted');

  let openOrders = await countOpenOrders('BTC', 'USDC', lp.address, chainflip);
  openOrders += await countOpenOrders('ETH', 'USDC', lp.address, chainflip);
  console.log(`Number of open orders: ${openOrders}`);

  console.log('Deleting all orders...');
  const orderDeleteEvent = observeEvent('liquidityPools:LimitOrderUpdated', {
    test: (event) => event.data.lp === lp.address && event.data.baseAsset === 'Btc',
  }).event;
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .cancelOrdersBatch(orderToDelete)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await orderDeleteEvent;
  console.log('All orders successfully deleted');

  openOrders = await countOpenOrders('BTC', 'USDC', lp.address, chainflip);
  openOrders += await countOpenOrders('ETH', 'USDC', lp.address, chainflip);
  console.log(`Number of open orders: ${openOrders}`);

  console.log(`Number of open orders: ${openOrders}`);
  console.log(`=== cancel_orders_batch test complete ===`);
}
