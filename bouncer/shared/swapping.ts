import { randomAsHex, randomAsNumber } from '@polkadot/util-crypto';
import { Asset, assetDecimals, Assets } from '@chainflip-io/cli';
import Web3 from 'web3';
import { performSwap, doPerformSwap, requestNewSwap, SenderType } from '../shared/perform_swap';
import {
  newAddress,
  chainFromAsset,
  getEthContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  observeBadEvents,
  observeFetch,
} from '../shared/utils';
import { BtcAddressType, btcAddressTypes } from '../shared/new_btc_address';
import { CcmDepositMetadata } from '../shared/new_swap';
import { performSwapViaContract, approveTokenVault } from '../shared/contract_swap';

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
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    ' Contract',
  );
  await performSwapViaContract(sourceAsset, destAsset, destAddress, tag, messageMetadata);
}

async function testDepositEthereum(sourceAsset: Asset, destAsset: Asset) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    ' AssetWitnessingTest',
  );

  const swapParams = await requestNewSwap(sourceAsset, destAsset, destAddress, tag);

  await doPerformSwap(swapParams, tag, undefined, SenderType.Contract);
  // Check the Deposit contract is deployed. It is assumed that the funds are fetched immediately.
  await observeFetch(sourceAsset, swapParams.depositAddress);
  await doPerformSwap(swapParams, tag, undefined, SenderType.Contract);
}

export async function testAllSwaps() {
  function appendSwap(
    swapArray: Promise<void>[],
    assetSource: Asset,
    assetDest: Asset,
    functionCall: (
      sourceAsset: Asset,
      destAsset: Asset,
      addressType?: BtcAddressType,
      messageMetadata?: CcmDepositMetadata,
    ) => Promise<void>,
    messageMetadata?: CcmDepositMetadata,
  ) {
    if (assetDest === 'BTC') {
      Object.values(btcAddressTypes).forEach((btcAddrType) => {
        // regularSwapsArray.push(testSwap(assetSource, assetDest, btcAddrType));
        swapArray.push(functionCall(assetSource, assetDest, btcAddrType, messageMetadata));
      });
    } else {
      swapArray.push(functionCall(assetSource, assetDest, undefined, messageMetadata));
    }
  }

  let stopObserving = false;
  const observingBadEvents = observeBadEvents(':BroadcastAborted', () => stopObserving);
  // Single approval of all the assets swapped in contractsSwaps to avoid overlapping async approvals.
  // Make sure to to set the allowance to the same amount of total asset swapped in contractsSwaps,
  // otherwise in subsequent approvals the broker might not send the transaction confusing the eth nonce.
  await approveTokenVault(
    'USDC',
    (BigInt(amountToFineAmount(defaultAssetAmounts('USDC'), assetDecimals.USDC)) * 9n).toString(),
  );
  await approveTokenVault(
    'FLIP',
    (BigInt(amountToFineAmount(defaultAssetAmounts('FLIP'), assetDecimals.FLIP)) * 9n).toString(),
  );

  const regularSwaps: Promise<void>[] = [];
  const contractSwaps: Promise<void>[] = [];
  const ccmSwaps: Promise<void>[] = [];
  const ccmContractSwaps: Promise<void>[] = [];

  Object.values(Assets).forEach((assetSource) => {
    Object.values(Assets).forEach((assetDest) => {
      // DO WE ALLOW SWAPS OF THE SAME CURRENCY???
      if (assetSource !== assetDest) {
        appendSwap(regularSwaps, assetSource, assetDest, testSwap);

        if (chainFromAsset(assetSource) === chainFromAsset('ETH')) {
          appendSwap(contractSwaps, assetSource, assetDest, testSwapViaContract);

          if (chainFromAsset(assetDest) === chainFromAsset('ETH')) {
            appendSwap(
              ccmContractSwaps,
              assetSource,
              assetDest,
              testSwapViaContract,
              newCcmMetadata(assetSource),
            );
          }
        }
        if (chainFromAsset(assetDest) === chainFromAsset('ETH')) {
          appendSwap(ccmSwaps, assetSource, assetDest, testSwap, newCcmMetadata(assetSource));
        }
      }
    });
  });

  const depositTestSwaps = Promise.all([
    testDepositEthereum('ETH', 'DOT'),
    testDepositEthereum('FLIP', 'BTC'),
  ]);

  await Promise.all([contractSwaps, regularSwaps, ccmSwaps, ccmContractSwaps, depositTestSwaps]);

  // Gracefully exit the broadcast abort observer
  stopObserving = true;
  await observingBadEvents;
}
