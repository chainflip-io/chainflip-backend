import { lpApiRpc } from './json_rpc';
import { createStateChainKeypair } from './utils';
import { Logger } from './utils/logger';
import { getChainflipApi } from './utils/substrate';

export async function DeleteAllOrdersLpApi(logger: Logger) {
  await using chainflip = await getChainflipApi();

  const lp = createStateChainKeypair(process.env.LP_URI || '//LP_1');

  let orders = await chainflip.rpc('cf_pool_orders', 'BTC', 'USDC', lp.address);
  logger.info(`BTC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'ETH', 'USDC', lp.address);
  logger.info(`ETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'DOT', 'USDC', lp.address);
  logger.info(`DOT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'ETH' },
    'USDC',
    lp.address,
  );
  logger.info(`ARBETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'USDC' },
    'USDC',
    lp.address,
  );
  logger.info(`ARBUSDC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'USDT' },
    'USDC',
    lp.address,
  );
  logger.info(`USDT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'FLIP' },
    'USDC',
    lp.address,
  );
  logger.info(`FLIP pool: ${JSON.stringify(orders)}`);

  await lpApiRpc(logger, `lp_cancel_all_orders`, []);

  orders = await chainflip.rpc('cf_pool_orders', 'BTC', 'USDC', lp.address);
  logger.info(`BTC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'ETH', 'USDC', lp.address);
  logger.info(`ETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'DOT', 'USDC', lp.address);
  logger.info(`DOT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'ETH' },
    'USDC',
    lp.address,
  );
  logger.info(`ARBETH pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Arbitrum', asset: 'USDC' },
    'USDC',
    lp.address,
  );
  logger.info(`ARBUSDC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'USDT' },
    'USDC',
    lp.address,
  );
  logger.info(`USDT pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc(
    'cf_pool_orders',
    { chain: 'Ethereum', asset: 'FLIP' },
    'USDC',
    lp.address,
  );
  logger.info(`FLIP pool: ${JSON.stringify(orders)}`);
}
