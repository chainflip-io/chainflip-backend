import { lpApiRpc } from 'shared/json_rpc';
import { createStateChainKeypair } from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { getChainflipApi } from 'shared/utils/substrate';

export async function DeleteAllOrdersLpApi(logger: Logger) {
  await using chainflip = await getChainflipApi();

  const lp = createStateChainKeypair(process.env.LP_URI || '//LP_1');

  let orders = await chainflip.rpc('cf_pool_orders', 'BTC', 'USDC', lp.address);
  logger.info(`BTC pool: ${JSON.stringify(orders)}`);
  orders = await chainflip.rpc('cf_pool_orders', 'ETH', 'USDC', lp.address);
  logger.info(`ETH pool: ${JSON.stringify(orders)}`);
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
