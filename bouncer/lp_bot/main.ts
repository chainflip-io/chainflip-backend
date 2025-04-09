#!/usr/bin/env -S pnpm tsx
import { initializeLiquidityProviderBot } from './lp_bot';
import { cancelAllOrdersForLp } from './utils';
import { globalLogger as logger } from '../shared/utils/logger';
import { startSwapSimulator } from './swap_simulator';

const main = async () => {

    const chain = 'Ethereum';
    const asset = 'USDT';

    await cancelAllOrdersForLp('//LP_API', chain, asset);
    await cancelAllOrdersForLp('//LP_1', chain, asset);
    await cancelAllOrdersForLp('//LP_2', chain, asset);

    const [stateChainWsConnection, lpWsConnection] = initializeLiquidityProviderBot(chain, asset);

    startSwapSimulator(100);

    process.on('SIGINT', () => {
        logger.info('Received SIGINT, closing connection');
        stateChainWsConnection.complete();
        lpWsConnection.complete();
        process.exit();
    });
};

main();