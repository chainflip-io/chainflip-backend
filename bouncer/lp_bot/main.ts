#!/usr/bin/env -S pnpm tsx
import { initializeLiquidityProviderBot, depositUsdcLiquidity, manageRangeOrder } from './lp_bot';
import { cancelAllOrdersForLp } from './utils';
import { globalLogger as logger } from '../shared/utils/logger';
import { startSwapSimulator } from './swap_simulator';

const main = async () => {
    await cancelAllOrdersForLp('//LP_API');
    await cancelAllOrdersForLp('//LP_1');
    await cancelAllOrdersForLp('//LP_2');

    const [stateChainWsConnection, lpWsConnection] = initializeLiquidityProviderBot();

    startSwapSimulator(100);

    process.on('SIGINT', () => {
        logger.info('Received SIGINT, closing connection');
        stateChainWsConnection.complete();
        lpWsConnection.complete();
        process.exit();
    });
};

main();