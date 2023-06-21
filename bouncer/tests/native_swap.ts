import { Asset } from '@chainflip-io/cli';
import { executeNativeSwap } from '../commands/native_swap';
import { chainflipApi, getAddress as newAddress, getBalanceSync, observeBalanceIncrease, observeEvent, runWithTimeout } from '../shared/utils';

async function testNativeSwap(destAsset: Asset) {

    const api = await chainflipApi();
    const addr = newAddress(destAsset, 'never');
    console.log("Destination address:", addr);

    const oldBalance = getBalanceSync(destAsset, addr);
    // Note that we start observing events before executing
    // the swap to avoid race conditions:
    console.log(`Executing native contract swap to (${destAsset}) ${addr}. Current balance: ${oldBalance}`)
    const handle = observeEvent("swapping:SwapExecuted", api);
    await executeNativeSwap(destAsset, addr);
    await handle;
    console.log(`Successfully observed event: swapping:SwapExecuted`);

    const newBalance = await observeBalanceIncrease(destAsset, addr, oldBalance);
    console.log(`Swap success! New balance: ${newBalance}`);
}

async function test() {
    await testNativeSwap('DOT');
    await testNativeSwap('USDC');
    await testNativeSwap('BTC');
}

// A successful execution usually takes ~150 seconds
runWithTimeout(test(), 180000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});