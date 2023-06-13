import { executeNativeSwap } from '../commands/native_swap';
import { chainflipApi, getBalanceSync, newDotAddress, observeBalanceIncrease, observeEvent, runWithTimeout } from '../shared/utils';

async function test() {

    const api = await chainflipApi();

    const addr = newDotAddress('1');

    const DST_CCY = 'dot'
    const oldBalance = getBalanceSync(DST_CCY, addr);

    // Note that we start observing events before executing
    // the swap to avoid race conditions:
    console.log(`Executing native swap to (${DST_CCY}) ${addr}. Current balance: ${oldBalance}`)
    const handle = observeEvent("swapping:SwapExecuted", api);
    await executeNativeSwap('DOT', addr);
    await handle;
    console.log(`Successfully observed event: swapping:SwapExecuted`);

    const newBalance = await observeBalanceIncrease(DST_CCY, addr, oldBalance);
    console.log(`Swap success! New balance: ${newBalance}`);
    // Don't wait for the timeout future to finish:
    process.exit(0);
}

// A successful execution usually takes ~150 seconds
runWithTimeout(test(), 180000).catch((error) => {
    console.error(error);
    process.exit(-1);
});