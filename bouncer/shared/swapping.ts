import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import Web3 from 'web3';
import assert from 'assert';
import { randomAsHex, randomAsNumber } from '../polkadot/util-crypto';
import { performSwap, SwapParams } from '../shared/perform_swap';
import {
  newAddress,
  chainFromAsset,
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  ccmSupportedChains,
  assetDecimals,
} from '../shared/utils';
import { BtcAddressType, btcAddressTypes } from '../shared/new_btc_address';
import { CcmDepositMetadata } from '../shared/new_swap';
import { performSwapViaContract, ContractSwapParams } from '../shared/contract_swap';

enum SolidityType {
  Uint256 = 'uint256',
  String = 'string',
  Bytes = 'bytes',
  Address = 'address',
}

export enum SwapStatus {
  Initiated,
  Funded,
  // Contract swap specific statuses
  ContractApproved,
  ContractExecuted,
  SwapScheduled,
  Success,
  Failure,
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
  const gasDiv = gasBudgetFraction ?? 2;

  const gasBudget = Math.floor(
    Number(amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals(sourceAsset))) /
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

  let tag = `[${(swapCount++).toString().concat(':').padEnd(4, ' ')} ${sourceAsset}->${destAsset}`;
  tag += messageMetadata ? ' CCM' : '';
  tag += tagSuffix ? `${tagSuffix}]` : ']';

  // For swaps with a message force the address to be the CF Tester address.
  if (messageMetadata && ccmSupportedChains.includes(chainFromAsset(destAsset))) {
    destAddress = getContractAddress(chainFromAsset(destAsset), 'CFTESTER');
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
  swapContext?: SwapContext,
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

  swapContext?.updateStatus(tag, SwapStatus.Initiated);

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
    swapContext,
  );
}
export async function testSwapViaContract(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  tagSuffix?: string,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    (tagSuffix ?? '') + ' Contract',
  );

  swapContext?.updateStatus(tag, SwapStatus.Initiated);
  return performSwapViaContract(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
    swapContext,
  );
}

export class SwapContext {
  allSwaps: Map<string, SwapStatus>;

  constructor() {
    this.allSwaps = new Map();
  }

  updateStatus(tag: string, status: SwapStatus) {
    const currentStatus = this.allSwaps.get(tag);

    // Sanity checks:
    switch (status) {
      case SwapStatus.Initiated: {
        assert(currentStatus === undefined, `Unexpected status transition for ${tag}`);
        break;
      }
      case SwapStatus.Funded: {
        assert(currentStatus === SwapStatus.Initiated, `Unexpected status transition for ${tag}`);
        break;
      }
      case SwapStatus.ContractApproved: {
        assert(currentStatus === SwapStatus.Initiated, `Unexpected status transition for ${tag}`);
        break;
      }
      case SwapStatus.ContractExecuted: {
        assert(
          currentStatus === SwapStatus.ContractApproved,
          `Unexpected status transition for ${tag}`,
        );
        break;
      }
      case SwapStatus.SwapScheduled: {
        assert(
          currentStatus === SwapStatus.ContractExecuted || currentStatus === SwapStatus.Funded,
          `Unexpected status transition for ${tag}`,
        );
        break;
      }
      case SwapStatus.Success: {
        assert(
          currentStatus === SwapStatus.SwapScheduled ||
            currentStatus === SwapStatus.ContractExecuted,
          `Unexpected status transition for ${tag}`,
        );
        break;
      }
      default:
        // nothing to do
        break;
    }

    this.allSwaps.set(tag, status);
  }

  print_report() {
    const unsuccessfulSwapsEntries: string[] = [];
    this.allSwaps.forEach((status, tag) => {
      if (status !== SwapStatus.Success) {
        unsuccessfulSwapsEntries.push(`${tag}: ${SwapStatus[status]}`);
      }
    });

    if (unsuccessfulSwapsEntries.length === 0) {
      console.log('All swaps are successful!');
    } else {
      let report = `Unsuccessful swaps:\n`;
      report += unsuccessfulSwapsEntries.join('\n');
      console.error(report);
    }
  }
}

export async function testAllSwaps(swapContext: SwapContext) {
  const allSwaps: Promise<SwapParams | ContractSwapParams>[] = [];

  function appendSwap(
    sourceAsset: Asset,
    destAsset: Asset,
    functionCall: typeof testSwap | typeof testSwapViaContract,
    messageMetadata?: CcmDepositMetadata,
  ) {
    if (destAsset === 'Btc') {
      Object.values(btcAddressTypes).forEach((btcAddrType) => {
        allSwaps.push(
          functionCall(sourceAsset, destAsset, btcAddrType, messageMetadata, swapContext),
        );
      });
    } else {
      allSwaps.push(functionCall(sourceAsset, destAsset, undefined, messageMetadata, swapContext));
    }
  }

  console.log('=== Testing all swaps ===');

  Object.values(Assets).forEach((sourceAsset) => {
    Object.values(Assets)
      .filter((destAsset) => sourceAsset !== destAsset)
      .forEach((destAsset) => {
        // Regular swaps
        appendSwap(sourceAsset, destAsset, testSwap);

        const sourceChain = chainFromAsset(sourceAsset);
        const destChain = chainFromAsset(destAsset);
        if ((sourceChain === 'Ethereum' || sourceChain === 'Arbitrum') && chainFromAsset(destAsset) !== 'Solana') {
          // Contract Swaps
          appendSwap(sourceAsset, destAsset, testSwapViaContract);
          if (destChain === 'Ethereum' || destChain === 'Arbitrum') {
            // CCM contract swaps
            appendSwap(sourceAsset, destAsset, testSwapViaContract, newCcmMetadata(sourceAsset));
          }
        }

        if (ccmSupportedChains.includes(destChain) && chainFromAsset(destAsset) !== 'Solana'){
          // CCM swaps
          appendSwap(sourceAsset, destAsset, testSwap, newCcmMetadata(sourceAsset));
        }
      });
  });

  // appendSwap("Eth", "Sol", testSwap, newCcmMetadata('Eth'))



  // ---------- CCM TESTING ---------------------------------------
  // function toHexString(byteArray: number[]) {
  //   return  '0x' +byteArray.map(function(byte) {
  //       return ('0' + (byte & 0xFF).toString(16)).slice(-2);
  //   }).join('');
  // }

  // let message = [124, 29, 15, 7];
  // let cf_parameters = [116, 23, 218, 139, 153, 215, 116, 129, 39, 167, 107, 3, 214, 31, 238, 105, 200, 13, 254, 247, 58, 210, 213, 80, 55, 55, 190, 237, 197, 169, 237, 72, 1, 4, 167, 59, 223, 49, 227, 65, 33, 138, 105, 59, 135, 114, 196, 62, 207, 206, 205, 76, 243, 95, 173, 160, 154, 135, 234, 15, 134, 13, 2, 129, 104, 229, 0];
  // // console.log("message", toHexString(message))
  // // console.log(newCcmMetadata('Eth').message)
  // // console.log("cf_params", toHexString(cf_parameters))
  // // console.log(newCcmMetadata('Eth').cfParameters)

  // appendSwap("Eth", "Sol", testSwap, {
  //   message: toHexString(message),
  //   gasBudget: newCcmMetadata('Eth').gasBudget,
  //   cfParameters:toHexString(cf_parameters),
  // })


  // ---------------------------------------------------------------
  // // TODO: For now we make sure to not do too many fetches are they
  // // will be batched obtaining a tx that is too long.
  // appendSwap('Sol', 'Eth', testSwap);
  // appendSwap('Sol', 'Usdc', testSwap);
  // appendSwap('Sol', 'Dot', testSwap);
  // appendSwap('SolUsdc', 'Eth', testSwap);
  // appendSwap('SolUsdc', 'Usdc', testSwap);
  // appendSwap('SolUsdc', 'Dot', testSwap);
  // appendSwap('Sol', 'Btc', testSwap);

  // TODO: For now do < 7 transfers to make sure we have nonces
  // available and it doesn't fail mid-building
  // appendSwap('Dot','Sol', testSwap);
  // appendSwap('Usdc','Sol', testSwap);
  // // appendSwap('Btc','Sol', testSwap);
  // appendSwap('Eth','Sol', testSwap);
  // // appendSwap('Eth','SolUsdc', testSwap);
  // appendSwap('Usdc','SolUsdc', testSwap);
  // appendSwap('Dot','SolUsdc', testSwap);
  // appendSwap('Btc','SolUsdc', testSwap);

  await Promise.all(allSwaps);

  console.log('=== Swapping test complete ===');
}
