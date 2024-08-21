import { Keyring } from '../polkadot/keyring';
import { handleSubstrateError, lpMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { limitOrder } from './limit_order';

export async function createAndDeleteAllOrders(numberOfOrders: number) {
  await using chainflip = await getChainflipApi();

  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const lpUri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  // create a series of limit_order and save their info to delete them later on
  const promises = [];
  const orderToDelete = [];
  let i = 0;
  while (i < numberOfOrders) {
    promises.push(limitOrder('Btc', 0.00000001, i, i));
    orderToDelete.push({ Limit: { base_asset: 'BTC', quote_asset: 'USDC', side: 'sell', id: i } });
    i++;
  }
  console.log('Submitting orders');
  await Promise.all(promises);
  console.log('Orders successfully submitted');

  let orders = await chainflip.rpc('cf_pool_orders', 'BTC', 'USDC', lp.address);
  let openOrders = 0;
  openOrders += orders.limit_orders.asks.length;
  openOrders += orders.limit_orders.bids.length;
  openOrders += orders.range_orders.length;
  console.log(`Number of open orders: ${openOrders}`);

  for (const order of orders.range_orders) {
    console.log(order);
    orderToDelete.push({
      Range: { base_asset: 'BTC', quote_asset: 'USDC', id: parseInt(order.id) },
    });
  }
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
  orders = await chainflip.rpc('cf_pool_orders', 'BTC', 'USDC', lp.address);
  console.log(orders);
  openOrders = 0;
  openOrders += orders.limit_orders.asks.length;
  openOrders += orders.limit_orders.bids.length;
  openOrders += orders.range_orders.length;
  console.log(`Number of open orders: ${openOrders}`);
}
