import { performSwapViaSmartContract } from '../shared/native_swap';
import { runWithTimeout } from '../shared/utils';

async function testAllNativeSwaps() {
    await Promise.all([performSwapViaSmartContract('DOT'), performSwapViaSmartContract('USDC'), performSwapViaSmartContract('BTC')]);
}

// A successful execution usually takes ~150 seconds
runWithTimeout(testAllNativeSwaps(), 180000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});