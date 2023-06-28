
import { randomAsHex } from "@polkadot/util-crypto";
import { Asset } from "@chainflip-io/cli/.";
import { performSwap } from "../shared/perform_swap";
import { getAddress, runWithTimeout } from "../shared/utils";
import { BtcAddressType } from "../shared/new_btc_address";
import { CcmDepositMetadata, ForeignChainAddress } from "../shared/new_swap";
import Web3 from 'web3';

let swapCount = 1;

async function testSwap(sourceToken: Asset, destToken: Asset, addressType?: BtcAddressType,  messageMetadata?: CcmDepositMetadata) {
    // Seed needs to be unique per swap:
    const seed = randomAsHex(32);
    let address = await getAddress(destToken, seed, addressType);
    
    // CCM to Ethereum happy path to the CF Receiver Mock. A CCM call to a random
    // address will fail so we force the address to be the CF Receiver Mock address.
    if (messageMetadata && (destToken === 'ETH' || destToken === 'USDC')) {
        address = "0xA51c1fc2f0D1a1b8494Ed1FE312d7C3a78Ed91C0"
    } 

    console.log(`Created new ${destToken} address: ${address}`);
    const tag = `[${swapCount++}: ${sourceToken}->${destToken}]`;

    await performSwap(sourceToken, destToken, address, tag, messageMetadata);
}

async function testAll() {
    const regularSwaps =
        Promise.all([
            testSwap('DOT', 'BTC', 'P2PKH'),
            testSwap('ETH', 'BTC', 'P2SH'),
            testSwap('USDC', 'BTC', 'P2WPKH'),
            testSwap('DOT', 'BTC', 'P2WSH'),
            testSwap('BTC', 'DOT'),
            testSwap('DOT', 'USDC'),
            testSwap('DOT', 'ETH'),
            testSwap('BTC', 'ETH'),
            testSwap('BTC', 'USDC'),

        ])
    
    // TODO: We can't do multiple CCM swaps in parallel from the same chain to ETH because
    // we get either the same deposit address or we get an undefined one. It is parsed by
    // dstAddress so since they both have the same dstAddress, the second one will fail.
    const ccmSwaps = 
        Promise.all([
            // NOTE: Seems like having a CCM swap and another without CCM with the same
            // src & dst chain fails. Needs to be investigated.
            testSwap('BTC', 'ETH', undefined, {
                message: 'BTC to ETH w/ CCM!!',
                gas_budget: 1000000,
                cf_parameters: [0],
                source_address: ForeignChainAddress.Bitcoin,
            }),
            // testSwap('BTC', 'USDC', undefined, {
            //     message: 'BTC to USDC w/ CCM!!',
            //     gas_budget: 1000000,
            //     cf_parameters: [0],
            //     source_address: ForeignChainAddress.Bitcoin,
            // }),
            // testSwap('BTC', 'ETH', undefined, {
            //     message: generateAbiEncodedMessage(),
            //     gas_budget: 1000000,
            //     cf_parameters: [0],
            //     source_address: ForeignChainAddress.Bitcoin,
            // }),            
            // testSwap('DOT', 'ETH', undefined, {
            //     message: 'DOT to ETH w/ CCM!!',
            //     gas_budget: 1000000,
            //     cf_parameters: [0],
            //     source_address: ForeignChainAddress.Polkadot,
            // }),            
        ])

    await Promise.all([regularSwaps, ccmSwaps]);
}


function generateAbiEncodedMessage(): string {
    const web3 = new Web3();
    return web3.eth.abi.encodeParameters(['uint256','string'], ['2345675643', 'Hello!%'])
}


runWithTimeout(testAll(), 1800000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});