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
      parseInt(amountToFineAmount(defaultAssetAmounts('USDC'), assetDecimals.USDC), 10) * 4
    ).toString(),
  );

  const ccmContractSwaps = Promise.all([
    testSwapViaContract('ETH', 'USDC', {
      message: getAbiEncodedMessage(['address', 'uint256', 'bytes']),
      gasBudget: 5000000,
      cfParameters: getAbiEncodedMessage(['address', 'uint256']),
    }),
    testSwapViaContract('USDC', 'ETH', {
      message: getAbiEncodedMessage(),
      gasBudget: 5000000,
      cfParameters: getAbiEncodedMessage(['bytes', 'uint256']),
    }),
  ]);

  const contractSwaps = Promise.all([
    testSwapViaContract('ETH', 'DOT'),
    testSwapViaContract('ETH', 'USDC'),
    testSwapViaContract('ETH', 'BTC'),
    testSwapViaContract('USDC', 'DOT'),
    testSwapViaContract('USDC', 'ETH'),
    testSwapViaContract('USDC', 'BTC'),
  ]);

  const regularSwaps = Promise.all([
    testSwap('ETH', 'BTC', 'P2PKH'),
    testSwap('ETH', 'BTC', 'P2WSH'),
    testSwap('ETH', 'BTC', 'P2SH'),
    testSwap('ETH', 'BTC', 'P2WPKH'),

    testSwap('USDC', 'BTC', 'P2PKH'),
    testSwap('USDC', 'BTC', 'P2WSH'),
    testSwap('USDC', 'BTC', 'P2SH'),
    testSwap('USDC', 'BTC', 'P2WPKH'),

    testSwap('BTC', 'ETH'),
    testSwap('BTC', 'USDC'),
    testSwap('BTC', 'DOT'),

    testSwap('DOT', 'ETH'),
    testSwap('DOT', 'USDC'),

    testSwap('DOT', 'BTC', 'P2PKH'),
    testSwap('DOT', 'BTC', 'P2WSH'),
    testSwap('DOT', 'BTC', 'P2SH'),
    testSwap('DOT', 'BTC', 'P2WPKH'),
  ]);

  // NOTE: Parallelized ccm swaps with the same sourceAsset and destAsset won't work because
  // all ccm swaps have the same destination address (cfReceiver) and then it will get a
  // potentially incorrect depositAddress.
  const ccmSwaps = Promise.all([
    // TODO: These two tests will be fixed in https://github.com/chainflip-io/chainflip-backend/pull/3708
    // testSwap('BTC', 'ETH', undefined, {
    //   message: new Web3().eth.abi.encodeParameter('string', 'BTC to ETH w/ CCM!!'),
    //   gasBudget: 1000000,
    //   cfParameters: '',
    // }),
    // testSwap('BTC', 'USDC', undefined, {
    //   message: '0x' + Buffer.from('BTC to ETH w/ CCM!!', 'ascii').toString('hex'),
    //   gasBudget: 600000,
    //   cfParameters: getAbiEncodedMessage(['uint256']),
    // }),
    testSwap('DOT', 'ETH', undefined, {
      message: getAbiEncodedMessage(['string', 'address']),
      gasBudget: 1000000,
      cfParameters: getAbiEncodedMessage(['string', 'string']),
    }),
    testSwap('DOT', 'USDC', undefined, {
      message: getAbiEncodedMessage(),
      gasBudget: 1000000,
      cfParameters: getAbiEncodedMessage(['address', 'uint256']),
    }),
    testSwap('USDC', 'ETH', undefined, {
      message: getAbiEncodedMessage(),
      gasBudget: 5000000,
      cfParameters: getAbiEncodedMessage(['bytes', 'uint256']),
    }),
    testSwap('ETH', 'USDC', undefined, {
      message: getAbiEncodedMessage(['address', 'uint256', 'bytes']),
      gasBudget: 5000000,
      cfParameters: getAbiEncodedMessage(['address', 'uint256']),
    }),
  ]);

  await Promise.all([contractSwaps, regularSwaps, ccmSwaps, ccmContractSwaps]);
}

runWithTimeout(testAll(), 1800000)
  .then(() => {
    // Don't wait for the timeout future to finish:
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
