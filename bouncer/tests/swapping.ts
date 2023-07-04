
import { randomAsHex } from "@polkadot/util-crypto";
import { Asset } from "@chainflip-io/cli/.";
import { performSwap } from "../shared/perform_swap";
import { getAddress, runWithTimeout, chainFromAsset, getEthContractAddress } from "../shared/utils";
import { BtcAddressType } from "../shared/new_btc_address";
import { CcmDepositMetadata, ForeignChainAddress } from "../shared/new_swap";
import { randomAsNumber } from "@polkadot/util-crypto";
import Web3 from 'web3';

let swapCount = 1;

async function testSwap(sourceToken: Asset, destToken: Asset, addressType?: BtcAddressType,  messageMetadata?: CcmDepositMetadata) {
    // Seed needs to be unique per swap:
    const seed = randomAsHex(32);
    let address = await getAddress(destToken, seed, addressType);
    
    // For swaps with a message force the address to be the CF Receiver Mock address.
    if (messageMetadata &&  chainFromAsset(destToken) === chainFromAsset('ETH')){
        address = getEthContractAddress('CFRECEIVER');
    }

    console.log(`Created new ${destToken} address: ${address}`);
    const tag = `[${swapCount++}: ${sourceToken}->${destToken}]`;

    await performSwap(sourceToken, destToken, address, tag, messageMetadata);
}

async function testAll() {
    await Promise.all([
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
    
    // NOTE: Doing the CCM swaps separately because of the broadcasting nonce bug.
    await Promise.all([
        testSwap('BTC', 'ETH', undefined, {
            message: new Web3().eth.abi.encodeParameter("string", "BTC to ETH w/ CCM!!"),
            gas_budget: 1000000,
            cf_parameters: "",
            source_address: ForeignChainAddress.Bitcoin,
        }),
    ])

    await Promise.all([
            testSwap('BTC', 'USDC', undefined, {
                message: '0x' + Buffer.from("BTC to ETH w/ CCM!!", 'ascii').toString('hex'),
                gas_budget: 600000,
                cf_parameters: getAbiEncodedMessage(["uint256"]),
                source_address: ForeignChainAddress.Bitcoin,
            }),
    ])

    await Promise.all([
            testSwap('BTC', 'ETH', undefined, {
                message: getAbiEncodedMessage(),
                gas_budget: 750000,
                cf_parameters: getAbiEncodedMessage([]),
                source_address: ForeignChainAddress.Bitcoin,
            }),       
    ])

    await Promise.all([
        testSwap('BTC', 'USDC', undefined, {
            message: getAbiEncodedMessage(["address","uint256","bytes"]),
            gas_budget: 2000000,
            cf_parameters: getAbiEncodedMessage(["string"]),
            source_address: ForeignChainAddress.Bitcoin,
        }),       
])

    await Promise.all([
            testSwap('DOT', 'ETH', undefined, {
                message: getAbiEncodedMessage(["string","address"]),
                gas_budget: 1000000,
                cf_parameters: getAbiEncodedMessage(["string","string"]),
                source_address: ForeignChainAddress.Polkadot,
            }),            
        ])
    await Promise.all([
        testSwap('DOT', 'USDC', undefined, {
            message: getAbiEncodedMessage(),
            gas_budget: 1000000,
            cf_parameters: getAbiEncodedMessage(["address","uint256"]),
            source_address: ForeignChainAddress.Polkadot,
        }),            
    ])        
    await Promise.all([
        testSwap('USDC', 'ETH', undefined, {
            message: getAbiEncodedMessage(),
            gas_budget: 5000000,
            cf_parameters: getAbiEncodedMessage(["address","uint256"]),
            source_address: ForeignChainAddress.Ethereum,
        }),            
    ])      
}


function getAbiEncodedMessage(types?: string[]): string {
    const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');

    const validSolidityTypes = ['uint256', 'string', 'bytes', 'address'];

    if (types === undefined) {
        types = [];
        for (let i = 0; i < Math.floor(Math.random() * validSolidityTypes.length) + 1; i++) {
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