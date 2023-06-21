
import { randomAsHex } from "@polkadot/util-crypto";
import { performSwap } from "../shared/perform_swap";
import { Token, getAddress, runWithTimeout } from "../shared/utils";
import { BtcAddressType } from "../shared/new_btc_address";

let swapCount = 1;

async function testSwap(sourceToken: Token, destToken: Token, addressType?: BtcAddressType) {
    // Seed needs to be unique per swap:
    const seed = randomAsHex(32);
    const address = await getAddress(destToken, seed, addressType);

    console.log(`Created new ${destToken} address: ${address}`);
    const tag = `[${swapCount++}: ${sourceToken}->${destToken}]`;

    await performSwap(sourceToken, destToken, address, tag);
}

async function testAll() {

    await Promise.all([
        testSwap('DOT', 'BTC', 'P2PKH'),
        testSwap('ETH', 'BTC', 'P2SH'),
        testSwap('USDC', 'BTC', 'P2WPKH'),
        testSwap('DOT', 'BTC', 'P2WSH'),
        testSwap('BTC', 'DOT'),
        testSwap('DOT', 'USDC'),
        testSwap('BTC', 'ETH'),
    ])

}

runWithTimeout(testAll(), 1800000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});