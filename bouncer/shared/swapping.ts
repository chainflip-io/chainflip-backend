import { InternalAsset as Asset } from '@chainflip/cli';
import { Keypair, PublicKey } from '@solana/web3.js';
import Web3 from 'web3';
import { u8aToHex } from '@polkadot/util';
import { randomAsHex, randomAsNumber } from '../polkadot/util-crypto';
import { performSwap } from '../shared/perform_swap';
import {
  newAddress,
  chainFromAsset,
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  ccmSupportedChains,
  assetDecimals,
  solCcmAdditionalDataCodec,
} from '../shared/utils';
import { BtcAddressType } from '../shared/new_btc_address';
import { CcmDepositMetadata } from '../shared/new_swap';
import { performSwapViaContract } from '../shared/contract_swap';
import { SwapContext, SwapStatus } from './swap_context';

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

export function newSolanaCcmAdditionalData(maxAccounts: number) {
  const cfReceiverAddress = getContractAddress('Solana', 'CFTESTER');

  const fallbackAddress = Keypair.generate().publicKey.toBytes();

  const remainingAccounts = [];
  const numRemainingAccounts = Math.floor(Math.random() * maxAccounts);

  for (let i = 0; i < numRemainingAccounts; i++) {
    remainingAccounts.push({
      pubkey: Keypair.generate().publicKey.toBytes(),
      is_writable: Math.random() < 0.5,
    });
  }

  const cfParameters = {
    cf_receiver: {
      pubkey: new PublicKey(cfReceiverAddress).toBytes(),
      is_writable: false,
    },
    remaining_accounts: remainingAccounts,
    fallback_address: fallbackAddress,
  };

  return u8aToHex(solCcmAdditionalDataCodec.enc(cfParameters));
}

// Solana CCM-related parameters. These are values in the protocol.
const maxCcmBytesSol = 705;
const maxCcmBytesUsdc = 492;
const bytesPerAccount = 33;

// Generate random bytes. Setting a minimum length of 10 because very short messages can end up
// with the SC returning an ASCII character in SwapDepositAddressReady.
function newCcmArbitraryBytes(maxLength: number): string {
  return randomAsHex(Math.floor(Math.random() * Math.max(0, maxLength - 10)) + 10);
}

function newCcmAdditionalData(destAsset: Asset, message?: string): string {
  const destChain = chainFromAsset(destAsset);
  switch (destChain) {
    case 'Ethereum':
    case 'Arbitrum':
      // Cf Parameters should be ignored by the protocol for any chain other than Solana
      return newCcmArbitraryBytes(100);
    case 'Solana': {
      const messageLength = (message!.length - 2) / 2;
      const maxAccounts = Math.floor(
        ((destAsset === 'Sol' ? maxCcmBytesSol : maxCcmBytesUsdc) - messageLength) /
          bytesPerAccount,
      );

      // The maximum number of extra accounts that can be passed is limited by the tx size
      // and therefore also depends on the message length.
      return newSolanaCcmAdditionalData(maxAccounts);
    }
    default:
      throw new Error(`Unsupported chain: ${destChain}`);
  }
}

function newCcmMessage(destAsset: Asset): string {
  const destChain = chainFromAsset(destAsset);
  switch (destChain) {
    case 'Ethereum':
    case 'Arbitrum':
      return newAbiEncodedMessage();
    case 'Solana':
      return newCcmArbitraryBytes(destAsset === 'Sol' ? maxCcmBytesSol : maxCcmBytesUsdc);
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
): CcmDepositMetadata {
  const message = ccmMessage ?? newCcmMessage(destAsset);
  const ccmAdditionalData = cfParamsArray ?? newCcmAdditionalData(destAsset, message);
  const gasDiv = gasBudgetFraction ?? 2;

  const gasBudget = Math.floor(
    Number(amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals(sourceAsset))) /
      gasDiv,
  ).toString();

  return {
    message,
    gasBudget,
    ccmAdditionalData,
  };
}

export async function prepareSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  tagSuffix?: string,
  log = true,
  swapContext?: SwapContext,
) {
  // Seed needs to be unique per swap:
  const seed = randomAsHex(32);

  let destAddress;

  let tag = `[${(swapCount++).toString().concat(':').padEnd(4, ' ')} ${sourceAsset}->${destAsset}`;
  tag += messageMetadata ? ' CCM' : '';
  tag += tagSuffix ? ` ${tagSuffix}]` : ']';

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

  swapContext?.updateStatus(tag, SwapStatus.Initiated);

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
    swapContext,
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
  log = true,
) {
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    (tagSuffix ?? '') + 'Contract',
    log,
    swapContext,
  );

  return performSwapViaContract(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
    swapContext,
    log,
  );
}
