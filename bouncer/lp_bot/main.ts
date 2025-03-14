#!/usr/bin/env -S pnpm tsx
import { initializeLiquidityProviderBot, depositUsdcLiquidity, cancelAllOrders, manageRangeOrder } from './lp_bot';
import { globalLogger as logger } from '../shared/utils/logger';
import { startSwapSimulator } from './swap_simulator';
import { Asset } from '../shared/utils';

const main = async () => {
    await cancelAllOrders();
    await depositUsdcLiquidity('100000');
    await manageRangeOrder('Usdt', 1, 10, 1000);

    const [stateChainWsConnection, lpWsConnection] = initializeLiquidityProviderBot();

    const abortController = new AbortController();
    startSwapSimulator(1000, abortController.signal);

    process.on('SIGINT', () => {
        logger.info('Received SIGINT, closing connection');
        stateChainWsConnection.complete();
        lpWsConnection.complete();
        abortController.abort();
        process.exit();
    });
};

main();