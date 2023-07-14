
import { randomAsHex, randomAsNumber } from "@polkadot/util-crypto";
import { Asset } from "@chainflip-io/cli/.";
import Web3 from 'web3';
import { performSwap } from "../shared/perform_swap";
import { getAddress, runWithTimeout, chainFromAsset, getEthContractAddress, encodeBtcAddressForContract, encodeDotAddressForContract } from "../shared/utils";
import { BtcAddressType } from "../shared/new_btc_address";
import { CcmDepositMetadata } from "../shared/new_swap";
import { performNativeSwap } from "../shared/native_swap";

let swapCount = 1;

async function testSwap(sourceToken: Asset, destToken: Asset, addressType?: BtcAddressType, messageMetadata?: CcmDepositMetadata) {
  // Seed needs to be unique per swap:
  const seed = randomAsHex(32);
  let address = await getAddress(destToken, seed, addressType);

  // For swaps with a message force the address to be the CF Receiver Mock address.
  if (messageMetadata && chainFromAsset(destToken) === chainFromAsset('ETH')) {
    address = getEthContractAddress('CFRECEIVER');
  }

  console.log(`Created new ${destToken} address: ${address}`);
  const tag = `[${swapCount++}: ${sourceToken}->${destToken}]`;

  await performSwap(sourceToken, destToken, address, tag, messageMetadata);
}

async function testAll() {
  const nativeContractSwaps = Promise.all([
    performNativeSwap('USDC'),
    performNativeSwap('BTC'),
  ]);

  const regularSwaps =
    Promise.all([
      testSwap('ETH', 'BTC', 'P2PKH'),
      testSwap('ETH', 'BTC', 'P2SH'),
      testSwap('USDC', 'BTC', 'P2WPKH'),
      testSwap('USDC', 'BTC', 'P2WSH'),
      testSwap('BTC', 'ETH'),
      testSwap('BTC', 'USDC'),
      testSwap('ETH', 'USDC'),
    ])

  await Promise.all([nativeContractSwaps, regularSwaps]);

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