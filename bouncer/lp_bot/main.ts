#!/usr/bin/env -S pnpm tsx
import { initializeLiquidityProviderBot } from './lp_bot';
import { globalLogger as logger } from '../shared/utils/logger';
// import { startSwapSimulator } from './swap_simulator';
// import { cancelAllOrdersForLp } from './utils';
// import { InternalAsset } from '@chainflip/cli';
// import { assetDecimals } from '../shared/utils';

const main = () => {
  const chain = 'Bitcoin';
  const asset = 'BTC';

  // await depositLiquidity(asset, '1000');

  // await cancelAllOrdersForLp('//LP_API', chain, asset);
  // await cancelAllOrdersForLp('//LP_1', chain, asset);
  // await cancelAllOrdersForLp('//LP_2', chain, asset);

  // await depositLiquidity(asset, '10000');

  const [stateChainWsConnection, lpWsConnection] = initializeLiquidityProviderBot(chain, asset);

  // startSwapSimulator(100);

  process.on('SIGINT', () => {
    logger.info('Received SIGINT, closing connection');
    stateChainWsConnection.complete();
    lpWsConnection.complete();
    process.exit();
  });
};

main();
