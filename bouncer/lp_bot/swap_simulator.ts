#!/usr/bin/env -S pnpm tsx

import { newAddress, sleep } from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';

import { globalLogger as logger } from '../shared/utils/logger';
import { sendBtc } from '../shared/send_btc';
import { sendErc20 } from '../shared/send_erc20';
import { getContractAddress } from '../shared/utils';



async function doSwap() {
    const destinationAddressForUsdc = await newAddress('Usdc', 'test');
    const refundAddress = await newAddress('Usdt', 'test');
    const refundParameters: FillOrKillParamsX128 = {
        retryDurationBlocks: 0,
        refundAddress,
        minPriceX128: '0',
    };

    const swapParams = await requestNewSwap(
        logger.child({ tag: 'swapSimulator' }),
        'Usdt',
        'Usdc',
        destinationAddressForUsdc,
        undefined,
        0,
        0,
        refundParameters,
    );

    const contractAddress = getContractAddress('Ethereum', 'Usdt');
    await sendErc20(logger, 'Ethereum', swapParams.depositAddress, contractAddress, '100');
    logger.info(`Usdt sent to ${swapParams.depositAddress}`);
}


async function main() {
    logger.info(`Lets swap!`);

    for (let i = 0; i < 10; i++) {
        logger.info(`Swapping ${i} times`);
        await doSwap();
        await sleep(2000);
    }
}

main();
