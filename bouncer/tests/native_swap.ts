import { executeNativeSwap } from '../commands/native_swap';
import { Token, chainflipApi, getAddress as newAddress, getBalanceSync, observeBalanceIncrease, observeEvent, runWithTimeout } from '../shared/utils';

async function testToken(destToken: Token) {

    const api = await chainflipApi();
    const addr = newAddress(destToken, 'never');
    console.log("Destination address:", addr);

    const oldBalance = getBalanceSync(destToken, addr);
    // Note that we start observing events before executing
    // the swap to avoid race conditions:
    console.log(`Executing native contract swap to (${destToken}) ${addr}. Current balance: ${oldBalance}`)
    const handle = observeEvent("swapping:SwapExecuted", api);
    await executeNativeSwap(destToken, addr);
    await handle;
    console.log(`Successfully observed event: swapping:SwapExecuted`);

    const newBalance = await observeBalanceIncrease(destToken, addr, oldBalance);
    console.log(`Swap success! New balance: ${newBalance}`);
}

async function test() {
    await testToken('DOT');
    await testToken('USDC');
}

// A successful execution usually takes ~150 seconds
runWithTimeout(test(), 180000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});