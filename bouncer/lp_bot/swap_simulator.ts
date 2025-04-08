import { newAddress, sleep } from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { globalLogger as logger } from '../shared/utils/logger';
import { sendErc20 } from '../shared/send_erc20';
import { getContractAddress } from '../shared/utils';

/**
 * Performs a swap.
 * 
 * @param amountToSwap - The amount of USDT to swap.
 */
async function doSwap(amountToSwap: string) {
    const destinationAddressForUsdc = await newAddress('Usdc', 'test');
    const refundAddress = await newAddress('Usdt', 'test');
    const refundParameters: FillOrKillParamsX128 = {
        retryDurationBlocks: 0,
        refundAddress,
        minPriceX128: '0',
    };
    logger.info(`Requesting new swap for ${amountToSwap} USDT...`);

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
    logger.info(`Sending ${amountToSwap} USDT to ${swapParams.depositAddress}...`);

    await sendErc20(logger, 'Ethereum', swapParams.depositAddress, contractAddress, amountToSwap);
    logger.info(`Usdt sent to ${swapParams.depositAddress}`);
}

/**
 * Starts the swap simulator.
 * 
 * @param limit - The number of swaps to perform.
 * @param signal - The signal to abort the swaps.
 */
async function startSwapSimulator(limit?: number) {
    logger.info(`Lets swap!`);

    if (limit) {
        logger.info(`Swapping ${limit} times!`);
    }

    let swaps = 0;

    while (true) {
        let amountToSwap = Math.floor(Math.random() * 1000);
        await doSwap(amountToSwap.toString());
        await sleep(2000);

        swaps++;
        if (limit && swaps >= limit) {
            logger.info('Swapping limit reached!');
            break;
        }
    }

    logger.info('Swap simulator completed!');
}

export { startSwapSimulator };
