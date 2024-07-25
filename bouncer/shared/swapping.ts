import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { Keypair, PublicKey } from '@solana/web3.js';
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

function newArbitraryBytes(maxLength: number): string {
  return randomAsHex(Math.floor(Math.random() * maxLength) + 1);
}

function newSolanaCfParameters(maxAccounts: number) {
  function arrayToHexString(byteArray: Uint8Array): string {
    return (
      '0x' +
      Array.from(byteArray)
        // eslint-disable-next-line no-bitwise
        .map((byte) => ('0' + (byte & 0xff).toString(16)).slice(-2))
        .join('')
    );
  }

  const cfReceiver = {
    pubkey: getContractAddress('Solana', 'CFTESTER'),
    is_writable: false,
  };

  // Convert the public keys and is_writable fields to byte arrays
  const cfReceiverBytes = new Uint8Array([
    ...new PublicKey(cfReceiver.pubkey).toBytes(),
    cfReceiver.is_writable ? 1 : 0,
  ]);

  const remainingAccounts = [];
  const numRemainingAccounts = Math.floor(Math.random() * maxAccounts);

  for (let i = 0; i < numRemainingAccounts; i++) {
    remainingAccounts.push(
      new Uint8Array([...Keypair.generate().publicKey.toBytes(), Math.random() < 0.5 ? 1 : 0]),
    );
  }

  // Concatenate the byte arrays
  const cfParameters = new Uint8Array([
    ...cfReceiverBytes,
    // Inserted by the codec::Encode
    4 * remainingAccounts.length,
    ...remainingAccounts.flatMap((account) => Array.from(account)),
  ]);

  return arrayToHexString(cfParameters);
}

// Solana CCM-related parameters. These are values in the protocol.
const maxCcmBytesSol = 705;
const maxCcmBytesUsdc = 492;
const bytesPerAccount = 33;

function newCcmMessageAndCfParameters(destAsset: Asset): { message: string; cfParameters: string } {
  const destChain = chainFromAsset(destAsset);
  switch (destChain) {
    case 'Ethereum':
    case 'Arbitrum':
      // Cf Parameters should be ignored by the protocol for any chain other than Solana
      return { message: newAbiEncodedMessage(), cfParameters: newArbitraryBytes(100) };
    case 'Solana': {
      const message = newArbitraryBytes(destAsset === 'Sol' ? maxCcmBytesSol : maxCcmBytesUsdc);
      const messageLength = (message.length - 2) / 2;

      const maxAccounts = Math.floor(
        ((destAsset === 'Sol' ? maxCcmBytesSol : maxCcmBytesUsdc) - messageLength) /
          bytesPerAccount,
      );
      // The maximum number of extra accounts that can be passed is limited by the tx size
      // and therefore also depends on the message length.
      return {
        message,
        cfParameters: newSolanaCfParameters(maxAccounts),
      };
    }
    default:
      throw new Error(`Unsupported chain: ${destChain}`);
  }
}

export function newCcmMetadata(
  sourceAsset: Asset,
  destAsset: Asset,
  ccmMessage?: string,
  gasBudgetFraction?: number,
  cfParamsArray?: string,
) {
  const ccmData = newCcmMessageAndCfParameters(destAsset);

  const message = ccmMessage ?? ccmData.message;
  const cfParameters = cfParamsArray ?? ccmData.cfParameters;
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
  if (
    messageMetadata &&
    ccmSupportedChains.includes(chainFromAsset(destAsset)) &&
    // Solana CCM are egressed to a random destination address
    chainFromAsset(destAsset) !== 'Solana'
  ) {
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
    ccmSwap: boolean = false,
  ) {
    if (destAsset === 'Btc') {
      const btcAddressTypesArray = Object.values(btcAddressTypes);
      allSwaps.push(
        functionCall(
          sourceAsset,
          destAsset,
          btcAddressTypesArray[Math.floor(Math.random() * btcAddressTypesArray.length)],
          ccmSwap ? newCcmMetadata(sourceAsset, destAsset) : undefined,
          swapContext,
        ),
      );
    } else {
      allSwaps.push(
        functionCall(
          sourceAsset,
          destAsset,
          undefined,
          ccmSwap ? newCcmMetadata(sourceAsset, destAsset) : undefined,
          swapContext,
        ),
      );
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
        if (sourceChain === 'Ethereum' || sourceChain === 'Arbitrum') {
          // Contract Swaps
          appendSwap(sourceAsset, destAsset, testSwapViaContract);

          if (ccmSupportedChains.includes(destChain)) {
            // CCM contract swaps
            appendSwap(sourceAsset, destAsset, testSwapViaContract, true);
          }
        }

        if (ccmSupportedChains.includes(destChain)) {
          // CCM swaps
          appendSwap(sourceAsset, destAsset, testSwap, true);
        }
      });
  });

  await Promise.all(allSwaps);

  console.log('=== Swapping test complete ===');
}
