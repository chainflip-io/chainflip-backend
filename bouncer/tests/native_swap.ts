import { performNativeSwap } from '../shared/native_swap';
import { runWithTimeout } from '../shared/utils';

async function testAllNativeSwaps() {
    await Promise.all([performNativeSwap('DOT'), performNativeSwap('USDC'), performNativeSwap('BTC')]);
}

// A successful execution usually takes ~150 seconds
runWithTimeout(testAllNativeSwaps(), 180000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});