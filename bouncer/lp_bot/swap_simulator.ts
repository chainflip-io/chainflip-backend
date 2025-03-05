#!/usr/bin/env -S pnpm tsx

import { newAddress } from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';

import { globalLogger as logger } from '../shared/utils/logger';
import { sendBtc } from '../shared/send_btc';

async function main() {
    console.log(`Lets swap!`);

    // Scedeule a BTC to ETH swap

    const destinationAddressForUsdc = await newAddress('Usdc', 'test');

    const refundAddress = await newAddress('Btc', 'test');

    const refundParameters: FillOrKillParamsX128 = {
        retryDurationBlocks: 0,
        refundAddress,
        minPriceX128: '0',
    };

    const swapParams = await requestNewSwap(
        logger.child({ tag: 'swapSimulator' }),
        'Btc',
        'Usdc',
        destinationAddressForUsdc,
        undefined,
        0,
        0,
        refundParameters,
    );

    const txId = await sendBtc(logger, swapParams.depositAddress, '0.0001', 0);

    console.log(`BTC sent to ${swapParams.depositAddress}`);
    console.log(`BTC txid: ${txId}`);
}

main();
