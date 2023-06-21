import { executeNativeSwap } from '../commands/native_swap';
import { getBalance } from '../shared/get_balance';
import { Token, getChainflipApi, getAddress as newAddress, observeBalanceIncrease, observeEvent, runWithTimeout } from '../shared/utils';

async function testNativeSwap(destToken: Token) {

    const api = await getChainflipApi();
    const addr = await newAddress(destToken, 'never');
    console.log("Destination address:", addr);

    const oldBalance = await getBalance(destToken, addr);
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