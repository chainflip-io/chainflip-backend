
import { randomAsHex , randomAsNumber } from "@polkadot/util-crypto";
import { Asset } from "@chainflip-io/cli/.";
import Web3 from 'web3';
import { performSwap } from "../shared/perform_swap";
import { getAddress, runWithTimeout, chainFromAsset, getEthContractAddress, encodeBtcAddressForContract, encodeDotAddressForContract } from "../shared/utils";
import { BtcAddressType } from "../shared/new_btc_address";
import { CcmDepositMetadata } from "../shared/new_swap";
import { performSwapViaContract, approveTokenVault } from "../shared/contract_swap";

let swapCount = 1;

async function testSwap(sourceToken: Asset, destToken: Asset, addressType?: BtcAddressType, messageMetadata?: CcmDepositMetadata) {
    // Seed needs to be unique per swap:
    const seed = randomAsHex(32);
    let address = await getAddress(destToken, seed, addressType);
    let tag = '[';
    // For swaps with a message force the address to be the CF Receiver Mock address.
    if (messageMetadata && chainFromAsset(destToken) === chainFromAsset('ETH')){
        tag += "CCM | "
        address = getEthContractAddress('CFRECEIVER');
        console.log(`${tag} Using CF Receiver Mock address: ${address}`);
    } else {
        address = await getAddress(destToken, seed, addressType);
        console.log(`${tag} Created new ${destToken} address: ${address}`);
    }

    console.log(`Created new ${destToken} address: ${address}`);
    tag += `${swapCount++}: ${sourceToken}->${destToken}]`;

    await performSwap(sourceToken, destToken, address, tag, messageMetadata);
}

async function testAll() {
    // Single approval of all the tokens swapped in contractsSwaps to avoid overlapping async approvals.
    await approveTokenVault('USDC', (500000000*3).toString());

    const contractSwaps = Promise.all([
        performSwapViaContract('ETH','DOT'),
        performSwapViaContract('ETH','USDC'),
        performSwapViaContract('ETH','BTC'),
        performSwapViaContract('USDC','DOT'),
        performSwapViaContract('USDC','ETH'),
        performSwapViaContract('USDC','BTC'),
    ]);

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
            testSwap('ETH', 'USDC'),
        ])

    // NOTE: Parallelized ccm swaps with the same sourceToken and destToken won't work because
    // all ccm swaps have the same destination address (cfReceiver) and then it will get a
    // potentially incorrect depositAddress.
    // DISABLED FOR NOW, UNTIL ENOUGH EVENT DATA IS AVAILABLE TO TEST THIS RELIABLY
    /*const ccmSwaps = Promise.all([
        testSwap('BTC', 'ETH', undefined, {
            message: new Web3().eth.abi.encodeParameter("string", "BTC to ETH w/ CCM!!"),
            gas_budget: 1000000,
            cf_parameters: "",
            source_address: { 'BTC': {'P2PKH': await getAddress('BTC', randomAsHex(32), 'P2PKH').then((btcAddress) => {encodeBtcAddressForContract(btcAddress)})}},
        }),
        testSwap('BTC', 'USDC', undefined, {
            message: '0x' + Buffer.from("BTC to ETH w/ CCM!!", 'ascii').toString('hex'),
            gas_budget: 600000,
            cf_parameters: getAbiEncodedMessage(["uint256"]),
            source_address: { 'BTC': {'P2SH': await getAddress('BTC', randomAsHex(32), 'P2SH').then((btcAddress) => {encodeBtcAddressForContract(btcAddress)})}},
        }),
        testSwap('DOT', 'ETH', undefined, {
            message: getAbiEncodedMessage(["string","address"]),
            gas_budget: 1000000,
            cf_parameters: getAbiEncodedMessage(["string","string"]),
            source_address: { 'DOT': await getAddress('DOT', randomAsHex(32)).then((dotAddress) => { encodeDotAddressForContract(dotAddress)})},
        }),            
        testSwap('DOT', 'USDC', undefined, {
            message: getAbiEncodedMessage(),
            gas_budget: 1000000,
            cf_parameters: getAbiEncodedMessage(["address","uint256"]),
            source_address: { 'DOT': await getAddress('DOT', randomAsHex(32)).then((dotAddress) => { encodeDotAddressForContract(dotAddress)})},
        }),            
        testSwap('USDC', 'ETH', undefined, {
            message: getAbiEncodedMessage(),
            gas_budget: 5000000,
            cf_parameters: getAbiEncodedMessage(["bytes","uint256"]),
            source_address: {'ETH': await getAddress('ETH', randomAsHex(32))},
        }),    
        testSwap('ETH', 'USDC', undefined, {
            message: getAbiEncodedMessage(["address","uint256","bytes"]),
            gas_budget: 5000000,
            cf_parameters: getAbiEncodedMessage(["address","uint256"]),
            source_address: {'ETH': await getAddress('ETH', randomAsHex(32))},
        })            
    ])*/

    await Promise.all([contractSwaps, regularSwaps/*, ccmSwaps*/]);

}


function getAbiEncodedMessage(types?: string[]): string {
    const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');

    const validSolidityTypes = ['uint256', 'string', 'bytes', 'address'];

    if (types === undefined) {
        types = [];
        const numElements = Math.floor(Math.random() * validSolidityTypes.length) + 1
        for (let i = 0; i < numElements; i++) {
            types.push(validSolidityTypes[Math.floor(Math.random() * validSolidityTypes.length)]);
          }
    }
    const variables: any[] = [];
    for (const type of types) {
      switch (type) {
        case 'uint256':
          variables.push(randomAsNumber());
          break;
        case 'string':
          variables.push(Math.random().toString(36).substring(2));
          break;
        case 'bytes':
          variables.push(randomAsHex(Math.floor(Math.random() * 100) + 1));
          break;
        case 'address':
          variables.push(randomAsHex(20));
          break;
        // Add more cases for other Solidity types as needed
        default:
          throw new Error(`Unsupported Solidity type: ${type}`);
      }
    }
    const encodedMessage = web3.eth.abi.encodeParameters(types, variables);
    return encodedMessage;
  }

runWithTimeout(testAll(), 1800000).then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
}).catch((error) => {
    console.error(error);
    process.exit(-1);
});