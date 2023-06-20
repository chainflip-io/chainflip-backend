
import { randomAsHex } from "@polkadot/util-crypto";
import { performSwap } from "../shared/perform_swap";
import { Token, getAddress, runWithTimeout } from "../shared/utils";
import { BtcAddressType } from "../shared/new_btc_address";

async function testSwap(sourceToken: Token, destToken: Token, addressType?: BtcAddressType) {
    // Seed needs to be unique per swap:
    const seed = randomAsHex(32);
    const address = await getAddress(destToken, seed, addressType);

    console.log(`Created new ${destToken} address: ${address}`);

    await performSwap(sourceToken, destToken, address);
}

async function testAll() {

    await testSwap('DOT', 'BTC', 'P2PKH');
    await testSwap('ETH', 'BTC', 'P2SH');
    await testSwap('USDC', 'BTC', 'P2WPKH');
    await testSwap('DOT', 'BTC', 'P2WSH');
    await testSwap('BTC', 'DOT');
    await testSwap('DOT', 'USDC');
    await testSwap('BTC', 'ETH');
}

runWithTimeout(testAll(), 1800000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});