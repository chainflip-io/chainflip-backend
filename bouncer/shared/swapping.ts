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

function newSolanaCfParameters() {
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
  const remainingAccount = {
    pubkey: Keypair.generate().publicKey,
    is_writable: false,
  };

  // Convert the public keys and is_writable fields to byte arrays
  const cfReceiverBytes = new Uint8Array([
    ...new PublicKey(cfReceiver.pubkey).toBytes(),
    cfReceiver.is_writable ? 1 : 0,
  ]);

  const remainingAccounts = [
    new Uint8Array([...remainingAccount.pubkey.toBytes(), remainingAccount.is_writable ? 1 : 0]),
  ];

  // Concatenate the byte arrays
  const cfParameters = new Uint8Array([
    ...cfReceiverBytes,
    // Inserted by the codec::Encode
    2 ** (remainingAccounts.length + 1),
    ...remainingAccounts.flatMap((account) => Array.from(account)),
  ]);
  return arrayToHexString(cfParameters);
}

function newCfParameters(destAsset: Asset) {
  const destChain = chainFromAsset(destAsset);
  switch (destChain) {
    case 'Ethereum':
    case 'Arbitrum':
      // Protocol shouldn't do anything with it.
      return newAbiEncodedMessage();
    case 'Solana':
      return newSolanaCfParameters();
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
  // const message = ccmMessage ?? newAbiEncodedMessage();

  function toHexStringFromArray(byteArray: Uint8Array) {
    return (
      '0x' +
      Array.from(byteArray)
        // eslint-disable-next-line no-bitwise
        .map((byte) => ('0' + (byte & 0xff).toString(16)).slice(-2))
        .join('')
    );
  }
  const message = toHexStringFromArray(new Uint8Array([124, 29, 15, 7]));

  const cfParameters = cfParamsArray ?? newCfParameters(destAsset);
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
    // Solana CCM are egressed at a random destination address
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
      Object.values(btcAddressTypes).forEach((btcAddrType) => {
        allSwaps.push(
          functionCall(
            sourceAsset,
            destAsset,
            btcAddrType,
            ccmSwap ? newCcmMetadata(sourceAsset, destAsset) : undefined,
            swapContext,
          ),
        );
      });
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

  // Object.values(Assets).forEach((sourceAsset) => {
  //   Object.values(Assets)
  //     .filter((destAsset) => sourceAsset !== destAsset)
  //     .forEach((destAsset) => {
  //       // Regular swaps
  //       // appendSwap(sourceAsset, destAsset, testSwap);

  //       const sourceChain = chainFromAsset(sourceAsset);
  //       const destChain = chainFromAsset(destAsset);
  //       // if (
  //       //   (sourceChain === 'Ethereum' || sourceChain === 'Arbitrum') &&
  //       //   chainFromAsset(destAsset) !== 'Solana'
  //       // ) {
  //       //   // Contract Swaps
  //       //   appendSwap(sourceAsset, destAsset, testSwapViaContract);
  //       //   if (destChain === 'Ethereum' || destChain === 'Arbitrum') {
  //       //     // CCM contract swaps
  //       //     appendSwap(
  //       //       sourceAsset,
  //       //       destAsset,
  //       //       testSwapViaContract,
  //       //       newCcmMetadata(sourceAsset, destAsset),
  //       //     );
  //       //   }
  //       // }

  //       if (
  //         ccmSupportedChains.includes(destChain) &&
  //         destChain === 'Solana' &&
  //       ) {
  //         // CCM swaps
  //         appendSwap(sourceAsset, destAsset, testSwap, newCcmMetadata(sourceAsset, destAsset));
  //       }
  //     });
  // });

  // TODO: More than 8 will cause aan egress fail (egressInvalid). CCM not retried?
  appendSwap('Eth', 'Sol', testSwap, true);
  appendSwap('Btc', 'Sol', testSwap, true);
  appendSwap('Dot', 'Sol', testSwap, true);
  appendSwap('ArbUsdc', 'Sol', testSwap, true);
  appendSwap('Usdc', 'SolUsdc', testSwap, true);
  appendSwap('Btc', 'SolUsdc', testSwap, true);
  appendSwap('Dot', 'SolUsdc', testSwap, true);
  appendSwap('ArbEth', 'SolUsdc', testSwap, true);

  await Promise.all(allSwaps);

  console.log('=== Swapping test complete ===');
}
