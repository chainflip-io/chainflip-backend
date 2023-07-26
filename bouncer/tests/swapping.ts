#!/usr/bin/env -S pnpm tsx
import { randomAsHex, randomAsNumber } from '@polkadot/util-crypto';
import { Asset, assetDecimals } from '@chainflip-io/cli';
import Web3 from 'web3';
import { performSwap } from '../shared/perform_swap';
import {
  getAddress,
  runWithTimeout,
  chainFromAsset,
  getEthContractAddress,
  encodeBtcAddressForContract,
  decodeDotAddressForContract,
  amountToFineAmount,
  defaultAssetAmounts,
} from '../shared/utils';
import { BtcAddressType } from '../shared/new_btc_address';
import { CcmDepositMetadata } from '../shared/new_swap';
import { performSwapViaContract, approveTokenVault } from '../shared/contract_swap';

let swapCount = 1;

function getAbiEncodedMessage(types?: string[]): string {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');

  const validSolidityTypes = ['uint256', 'string', 'bytes', 'address'];
  let typesArray: string[] = [];
  if (types === undefined) {
    const numElements = Math.floor(Math.random() * validSolidityTypes.length) + 1;
    for (let i = 0; i < numElements; i++) {
      typesArray.push(validSolidityTypes[Math.floor(Math.random() * validSolidityTypes.length)]);
    }
  } else {
    typesArray = types;
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const variables: any[] = [];

  for (let i = 0; i < typesArray.length; i++) {
    switch (typesArray[i]) {
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
        throw new Error(`Unsupported Solidity type: ${typesArray[i]}`);
    }
  }
  const encodedMessage = web3.eth.abi.encodeParameters(typesArray, variables);
  return encodedMessage;
}

export async function prepareSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  tagSuffix?: string,
) {
  // Seed needs to be unique per swap:
  const seed = randomAsHex(32);

  let destAddress;

  let tag = `[${(swapCount++).toString().padEnd(2, ' ')}: ${sourceAsset}->${destAsset}`;
  tag += messageMetadata ? ' CCM' : '';
  tag += tagSuffix ? `${tagSuffix}]` : ']';

  // For swaps with a message force the address to be the CF Receiver Mock address.
  if (messageMetadata && chainFromAsset(destAsset) === chainFromAsset('ETH')) {
    destAddress = getEthContractAddress('CFRECEIVER');
    console.log(`${tag} Using CF Receiver Mock address: ${destAddress}`);
  } else {
    destAddress = await getAddress(destAsset, seed, addressType);
    console.log(`${tag} Created new ${destAsset} address: ${destAddress}`);
  }

  return { destAddress, tag };
}

async function testSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
  );
  await performSwap(sourceAsset, destAsset, destAddress, tag, messageMetadata);
}

async function testSwapViaContract(
  sourceAsset: Asset,
  destAsset: Asset,
  messageMetadata?: CcmDepositMetadata,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    undefined,
    messageMetadata,
    ' Contract',
  );
  await performSwapViaContract(sourceAsset, destAsset, destAddress, tag, messageMetadata);
}

async function testAll() {
  // Single approval of all the assets swapped in contractsSwaps to avoid overlapping async approvals.
  // Make sure to to set the allowance to the same amount of total asset swapped in contractsSwaps,
  // otherwise in subsequent approvals the broker might not send the transaction confusing the eth nonce.
  await approveTokenVault(
    'USDC',
    (
      BigInt(amountToFineAmount(defaultAssetAmounts('USDC'), assetDecimals.USDC)) * BigInt(6)
    ).toString(),
  );
  await approveTokenVault(
    'FLIP',
    (
      BigInt(amountToFineAmount(defaultAssetAmounts('FLIP'), assetDecimals.FLIP)) * BigInt(6)
    ).toString(),
  );

  const ccmContractSwaps = Promise.all([
    testSwapViaContract('ETH', 'USDC', {
      message: getAbiEncodedMessage(['address', 'uint256', 'bytes']),
      gas_budget: 5000000,
      cf_parameters: getAbiEncodedMessage(['address', 'uint256']),
      source_address: { ETH: await getAddress('ETH', randomAsHex(32)) },
    }),
    testSwapViaContract('USDC', 'ETH', {
      message: getAbiEncodedMessage(),
      gas_budget: 5000000,
      cf_parameters: getAbiEncodedMessage(['bytes', 'uint256']),
      source_address: { ETH: await getAddress('ETH', randomAsHex(32)) },
    }),
    testSwapViaContract('FLIP', 'ETH', {
      message: getAbiEncodedMessage(),
      gas_budget: 10000000000000000,
      cf_parameters: getAbiEncodedMessage(['bytes', 'uint256']),
      source_address: { ETH: await getAddress('ETH', randomAsHex(32)) },
    }),
  ]);

  const contractSwaps = Promise.all([
    testSwapViaContract('ETH', 'DOT'),
    testSwapViaContract('ETH', 'USDC'),
    testSwapViaContract('ETH', 'BTC'),
    testSwapViaContract('ETH', 'FLIP'),
    testSwapViaContract('USDC', 'DOT'),
    testSwapViaContract('USDC', 'ETH'),
    testSwapViaContract('USDC', 'BTC'),
    testSwapViaContract('USDC', 'FLIP'),
    testSwapViaContract('FLIP', 'DOT'),
    testSwapViaContract('FLIP', 'ETH'),
    testSwapViaContract('FLIP', 'BTC'),
    testSwapViaContract('FLIP', 'USDC'),
  ]);

  const regularSwaps = Promise.all([
    testSwap('DOT', 'BTC', 'P2PKH'),
    testSwap('ETH', 'BTC', 'P2SH'),
    testSwap('USDC', 'BTC', 'P2WPKH'),
    testSwap('DOT', 'BTC', 'P2WSH'),
    testSwap('FLIP', 'BTC', 'P2SH'),
    testSwap('BTC', 'DOT'),
    testSwap('DOT', 'USDC'),
    testSwap('DOT', 'ETH'),
    testSwap('BTC', 'ETH'),
    testSwap('BTC', 'USDC'),
    testSwap('ETH', 'USDC'),
    testSwap('FLIP', 'DOT'),
    testSwap('BTC', 'FLIP'),
  ]);

  // NOTE: Parallelized ccm swaps with the same sourceAsset and destAsset won't work because
  // all ccm swaps have the same destination address (cfReceiver) and then it will get a
  // potentially incorrect depositAddress.
  const ccmSwaps = Promise.all([
    testSwap('BTC', 'ETH', undefined, {
      message: new Web3().eth.abi.encodeParameter('string', 'BTC to ETH w/ CCM!!'),
      gas_budget: 1000000,
      cf_parameters: '',
      source_address: {
        BTC: {
          P2PKH: await getAddress('BTC', randomAsHex(32), 'P2PKH').then((btcAddress) => {
            encodeBtcAddressForContract(btcAddress);
          }),
        },
      },
    }),
    testSwap('BTC', 'USDC', undefined, {
      message: '0x' + Buffer.from('BTC to ETH w/ CCM!!', 'ascii').toString('hex'),
      gas_budget: 600000,
      cf_parameters: getAbiEncodedMessage(['uint256']),
      source_address: {
        BTC: {
          P2SH: await getAddress('BTC', randomAsHex(32), 'P2SH').then((btcAddress) => {
            encodeBtcAddressForContract(btcAddress);
          }),
        },
      },
    }),
    testSwap('DOT', 'ETH', undefined, {
      message: getAbiEncodedMessage(['string', 'address']),
      gas_budget: 1000000,
      cf_parameters: getAbiEncodedMessage(['string', 'string']),
      source_address: {
        DOT: await getAddress('DOT', randomAsHex(32)).then((dotAddress) => {
          decodeDotAddressForContract(dotAddress);
        }),
      },
    }),
    testSwap('DOT', 'FLIP', undefined, {
      message: getAbiEncodedMessage(),
      gas_budget: 1000000,
      cf_parameters: getAbiEncodedMessage(['address', 'uint256']),
      source_address: {
        DOT: await getAddress('DOT', randomAsHex(32)).then((dotAddress) => {
          decodeDotAddressForContract(dotAddress);
        }),
      },
    }),
    testSwap('USDC', 'ETH', undefined, {
      message: getAbiEncodedMessage(),
      gas_budget: 5000000,
      cf_parameters: getAbiEncodedMessage(['bytes', 'uint256']),
      source_address: { ETH: await getAddress('ETH', randomAsHex(32)) },
    }),
    testSwap('ETH', 'USDC', undefined, {
      message: getAbiEncodedMessage(['address', 'uint256', 'bytes']),
      gas_budget: 5000000,
      cf_parameters: getAbiEncodedMessage(['address', 'uint256']),
      source_address: { ETH: await getAddress('ETH', randomAsHex(32)) },
    }),
  ]);

  await Promise.all([contractSwaps, regularSwaps, ccmSwaps, ccmContractSwaps]);
}

runWithTimeout(testAll(), 1800000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
