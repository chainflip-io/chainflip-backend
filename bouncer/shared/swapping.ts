import { randomAsHex, randomAsNumber } from '@polkadot/util-crypto';
import { Asset, assetDecimals, Assets } from '@chainflip-io/cli';
import Web3 from 'web3';
import { performSwap, SwapParams } from '../shared/perform_swap';
import {
  newAddress,
  chainFromAsset,
  getEvmContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  assetToChain,
  ccmSupportedChains,
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
  const web3 = new Web3();

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
  return web3.eth.abi.encodeParameters(typesArray, variables);
}

export function newCcmMetadata(
  sourceAsset: Asset,
  ccmMessage?: string,
  gasBudgetFraction?: number,
  cfParamsArray?: SolidityType[],
) {
  const message = ccmMessage ?? newAbiEncodedMessage();
  const cfParameters = newAbiEncodedMessage(cfParamsArray);
  // TODO: Arbitrum has way higher fees, to check if this works. This should depend on
  // the egress chain but it's annoying to get that value here. We'd rather set a fraction
  // that works for all chains.
  const gasDiv = gasBudgetFraction ?? 2;

  const gasBudget = Math.floor(
    Number(amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals[sourceAsset])) /
      gasDiv,
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
  log = true,
) {
  // Seed needs to be unique per swap:
  const seed = randomAsHex(32);

  let destAddress;

  let tag = `[${(swapCount++).toString().padEnd(2, ' ')}: ${sourceAsset}->${destAsset}`;
  tag += messageMetadata ? ' CCM' : '';
  tag += tagSuffix ? `${tagSuffix}]` : ']';

  // For swaps with a message force the address to be the CF Tester address.
  if (messageMetadata && ccmSupportedChains.includes(chainFromAsset(destAsset))) {
    destAddress = getEvmContractAddress(chainFromAsset(destAsset), 'CFTESTER');
    if (log) console.log(`${tag} Using CF Tester address: ${destAddress}`);
  } else {
    destAddress = await newAddress(destAsset, seed, addressType);
    if (log) console.log(`${tag} Created new ${destAsset} address: ${destAddress}`);
  }

  return { destAddress, tag };
}

export async function testSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  tagSuffix?: string,
  amount?: string,
  log = true,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    tagSuffix,
    log,
  );
  return performSwap(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
    undefined,
    amount,
    undefined,
    log,
  );
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
  const allSwaps: Promise<SwapParams | ContractSwapParams>[] = [];

  function appendSwap(
    sourceAsset: Asset,
    destAsset: Asset,
    functionCall: typeof testSwap | typeof testSwapViaContract,
    messageMetadata?: CcmDepositMetadata,
  ) {
    if (destAsset === 'BTC') {
      Object.values(btcAddressTypes).forEach((btcAddrType) => {
        allSwaps.push(functionCall(sourceAsset, destAsset, btcAddrType, messageMetadata));
      });
    } else {
      allSwaps.push(functionCall(sourceAsset, destAsset, undefined, messageMetadata));
    }
  }

  console.log('=== Testing all swaps ===');

  // TODO: Manually do token approvals and contract swaps.

  // Single approval of all the assets swapped in contractsSwaps to avoid overlapping async approvals.
  // Set the allowance to the same amount of total asset swapped in contractsSwaps to avoid nonce issues.
  // Total contract swap per ERC20 token = ccmContractSwaps + contractSwaps =
  //     (numberAssetsEthereum - 1) + (numberAssets (BTC has 4 different types) - 1) = 2 + 7 = 9
  // await approveTokenVault(
  //   'USDC',
  //   (BigInt(amountToFineAmount(defaultAssetAmounts('USDC'), assetDecimals.USDC)) * 9n).toString(),
  // );
  // await approveTokenVault(
  //   'FLIP',
  //   (BigInt(amountToFineAmount(defaultAssetAmounts('FLIP'), assetDecimals.FLIP)) * 9n).toString(),
  // );

  // NOTE: Sometimes getting this error. I think it's related to starting too many connections in parallel to the SC.
  // This won't be a problem when the broker supports it.
  //    RPC-CORE: getMetadata(at?: BlockHash): Metadata:: -32000: Client error: Execution failed: Other error happened while constructing the runtime: failed to instantiate a new WASM module instance: maximum concurrent instance limit of 32 reached
  //    API/INIT: Error: FATAL: Unable to initialize the API: -32000: Client error: Execution failed: Other error happened while constructing the runtime: failed to instantiate a new WASM module instance: maximum concurrent instance limit of 32 reached
  //              at ApiPromise.__internal__onProviderConnect (file:///home/albert/work/chainflip/backend_arbitrum/chainflip-backend/bouncer/node_modules/.pnpm/@polkadot+api@10.7.2/node_modules/@polkadot/api/base/Init.js:311:27)
  //              at processTicksAndRejections (node:internal/process/task_queues:96:5)

  Object.values(Assets).forEach((sourceAsset) =>
    Object.values(Assets)
      .filter((destAsset) => sourceAsset !== destAsset)
      .forEach((destAsset) => {
        // Regular swaps
        appendSwap(sourceAsset, destAsset, testSwap);

        // // NOTE: I am using an old SDK so this ones don't work, even for non-Arbitrum assets
        // // if (sourceAsset !== 'ARBETH' && sourceAsset !== 'ARBUSDC') {
        // if (chainFromAsset(sourceAsset) === chainFromAsset('ETH')) {
        //   // Contract Swaps
        //   appendSwap(sourceAsset, destAsset, testSwapViaContract);
        //   if (chainFromAsset(destAsset) === chainFromAsset('ETH')) {
        //     // CCM contract swaps
        //     appendSwap(sourceAsset, destAsset, testSwapViaContract, newCcmMetadata(sourceAsset));
        //   }
        // }

        // if (ccmSupportedChains.includes(chainFromAsset(destAsset))) {
        //   // CCM swaps
        //   appendSwap(sourceAsset, destAsset, testSwap, newCcmMetadata(sourceAsset));
        // }
      }),
  );

  // appendSwap('ETH', 'ARBETH', testSwap, newCcmMetadata('ETH'));
  // appendSwap('ETH', 'ARBETH', testSwap);

  await Promise.all(allSwaps);

  console.log('=== Swapping test complete ===');
}
