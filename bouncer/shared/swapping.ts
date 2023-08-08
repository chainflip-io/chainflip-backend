import { randomAsHex, randomAsNumber } from '@polkadot/util-crypto';
import { Asset, assetDecimals, Assets } from '@chainflip-io/cli';
import Web3 from 'web3';
import { performSwap, SwapParams } from '../shared/perform_swap';
import {
  newAddress,
  chainFromAsset,
  getEthContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
} from '../shared/utils';
import { BtcAddressType, btcAddressTypes } from '../shared/new_btc_address';
import { CcmDepositMetadata } from '../shared/new_swap';
import {
  performSwapViaContract,
  approveTokenVault,
  ContractSwapParams,
} from '../shared/contract_swap';

enum SolidityType {
  Uint256 = 'uint256',
  String = 'string',
  Bytes = 'bytes',
  Address = 'address',
}

let swapCount = 1;

function newAbiEncodedMessage(types?: SolidityType[]): string {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');

  let typesArray: SolidityType[] = [];
  if (types === undefined) {
    const numElements = Math.floor(Math.random() * (Object.keys(SolidityType).length / 2)) + 1;
    for (let i = 0; i < numElements; i++) {
      typesArray.push(
        Object.values(SolidityType)[
          Math.floor(Math.random() * (Object.keys(SolidityType).length / 2))
        ],
      );
    }
  } else {
    typesArray = types;
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const variables: any[] = [];

  for (let i = 0; i < typesArray.length; i++) {
    switch (typesArray[i]) {
      case SolidityType.Uint256:
        variables.push(randomAsNumber());
        break;
      case SolidityType.String:
        variables.push(Math.random().toString(36).substring(2));
        break;
      case SolidityType.Bytes:
        variables.push(randomAsHex(Math.floor(Math.random() * 100) + 1));
        break;
      case SolidityType.Address:
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

function newCcmMetadata(
  sourceAsset: Asset,
  gas?: number,
  messageTypesArray?: SolidityType[],
  cfParamsArray?: SolidityType[],
) {
  const message = newAbiEncodedMessage(messageTypesArray);
  const cfParameters = newAbiEncodedMessage(cfParamsArray);
  const gasBudget =
    gas ??
    Math.floor(
      Number(amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals[sourceAsset])) /
        100,
    );

  return {
    message,
    gasBudget,
    cfParameters,
  };
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

  // For swaps with a message force the address to be the CF Tester address.
  if (messageMetadata && chainFromAsset(destAsset) === chainFromAsset('ETH')) {
    destAddress = getEthContractAddress('CFTESTER');
    console.log(`${tag} Using CF Tester address: ${destAddress}`);
  } else {
    destAddress = await newAddress(destAsset, seed, addressType);
    console.log(`${tag} Created new ${destAsset} address: ${destAddress}`);
  }

  return { destAddress, tag };
}

export async function testSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  tagSuffix?: string,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    tagSuffix,
  );
  return performSwap(sourceAsset, destAsset, destAddress, tag, messageMetadata);
}
async function testSwapViaContract(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  tagSuffix?: string,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    (tagSuffix ?? '') + ' Contract',
  );
  return performSwapViaContract(sourceAsset, destAsset, destAddress, tag, messageMetadata);
}

export async function testAllSwaps() {
  function appendSwap(
    swapArray: Promise<SwapParams | ContractSwapParams>[],
    sourceAsset: Asset,
    destAsset: Asset,
    functionCall: (
      sourceAsset: Asset,
      destAsset: Asset,
      addressType?: BtcAddressType,
      messageMetadata?: CcmDepositMetadata,
      tagSuffix?: string,
    ) => Promise<SwapParams | ContractSwapParams>,
    messageMetadata?: CcmDepositMetadata,
  ) {
    if (destAsset === 'BTC') {
      Object.values(btcAddressTypes).forEach((btcAddrType) => {
        swapArray.push(functionCall(sourceAsset, destAsset, btcAddrType, messageMetadata));
      });
    } else {
      swapArray.push(functionCall(sourceAsset, destAsset, undefined, messageMetadata));
    }
  }

  console.log('=== Testing all swaps ===');

  // Single approval of all the assets swapped in contractsSwaps to avoid overlapping async approvals.
  // Set the allowance to the same amount of total asset swapped in contractsSwaps to avoid nonce issues.
  // Total contract swap per ERC20 token = ccmContractSwaps + contractSwaps =
  //     (numberAssetsEthereum - 1) + (numberAssets (BTC has 4 different types) - 1) = 2 + 7 = 9
  await approveTokenVault(
    'USDC',
    (BigInt(amountToFineAmount(defaultAssetAmounts('USDC'), assetDecimals.USDC)) * 9n).toString(),
  );
  await approveTokenVault(
    'FLIP',
    (BigInt(amountToFineAmount(defaultAssetAmounts('FLIP'), assetDecimals.FLIP)) * 9n).toString(),
  );

  const allSwaps: Promise<SwapParams | ContractSwapParams>[] = [];

  Object.values(Assets).forEach((sourceAsset) => {
    Object.values(Assets).forEach((destAsset) => {
      // SDK prevents swaps from the same asset to the same asset
      if (sourceAsset !== destAsset) {
        // Regular swaps
        appendSwap(allSwaps, sourceAsset, destAsset, testSwap);

        if (chainFromAsset(sourceAsset) === chainFromAsset('ETH')) {
          // Contract Swaps
          appendSwap(allSwaps, sourceAsset, destAsset, testSwapViaContract);

          if (chainFromAsset(destAsset) === chainFromAsset('ETH')) {
            // CCM contract swaps
            appendSwap(
              allSwaps,
              sourceAsset,
              destAsset,
              testSwapViaContract,
              newCcmMetadata(sourceAsset),
            );
          }
        }
        if (chainFromAsset(destAsset) === chainFromAsset('ETH')) {
          // CCM swaps
          appendSwap(allSwaps, sourceAsset, destAsset, testSwap, newCcmMetadata(sourceAsset));
        }
      }
    });
  });

  await Promise.all(allSwaps);
}
