import { lpApiRpc } from '../tests/lp_api_test';
import { createStateChainKeypair } from './utils';
import { getChainflipApi } from './utils/substrate';

export async function DeleteAllOrdersLpApi() {
  await using chainflip = await getChainflipApi();

  const lp = createStateChainKeypair(process.env.LP_URI || '//LP_1');

  let orders = await chainflip.rpc('cf_pool_orders', 'BTC', 'USDC', lp.address);
  console.log(`BTC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'ETH', 'USDC', lp.address);
  console.log(`ETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'DOT', 'USDC', lp.address);
  console.log(`DOT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'ETH' },
    'USDC',
    lp.address,
  );
  console.log(`ARBETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'USDC' },
    'USDC',
    lp.address,
  );
  console.log(`ARBUSDC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'USDT' },
    'USDC',
    lp.address,
  );
  console.log(`USDT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'FLIP' },
    'USDC',
    lp.address,
  );
  console.log(`FLIP pool: ${JSON.stringify(orders)}`);

  await lpApiRpc(`lp_cancel_all_orders`, []);

  orders = await chainflip.rpc('cf_pool_orders', 'BTC', 'USDC', lp.address);
  console.log(`BTC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'ETH', 'USDC', lp.address);
  console.log(`ETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'DOT', 'USDC', lp.address);
  console.log(`DOT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'ETH' },
    'USDC',
    lp.address,
  );
  console.log(`ARBETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'USDC' },
    'USDC',
    lp.address,
  );
  console.log(`ARBUSDC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'USDT' },
    'USDC',
    lp.address,
  );
  console.log(`USDT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'FLIP' },
    'USDC',
    lp.address,
  );
  console.log(`FLIP pool: ${JSON.stringify(orders)}`);
}
