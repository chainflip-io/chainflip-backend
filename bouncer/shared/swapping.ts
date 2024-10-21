import { InternalAsset as Asset } from '@chainflip/cli';
import { Keypair, PublicKey } from '@solana/web3.js';
import Web3 from 'web3';
import { u32, Struct, Option, u16, u256, Bytes as TsBytes, Enum } from 'scale-ts';
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

function newSolanaCcmAdditionalData(maxAccounts: number) {
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

  const fallbackAddrBytes = new PublicKey(getContractAddress('Solana', 'FALLBACK')).toBytes();

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
    ...fallbackAddrBytes,
  ]);

  return arrayToHexString(cfParameters);
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

export const vaultSwapCfParametersCodec = Struct({
  ccmAdditionalData: Option(TsBytes()),
  vaultSwapAttributes: Option(
    Struct({
      refundParams: Option(
        Struct({
          retryDurationBlocks: u32,
          refundAddress: Enum({
            Eth: TsBytes(20),
            Dot: TsBytes(32), // not supported anyway
            Btc: TsBytes(), // not supported anyway
            Arb: TsBytes(20),
            Sol: TsBytes(32),
          }),
          minPriceX128: u256,
        }),
      ),
      dcaParams: Option(Struct({ numberOfChunks: u32, chunkIntervalBlocks: u32 })),
      boostFee: Option(u16),
    }),
  ),
});

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
    // TODO: To rename to ccmAdditionalData
    cfParameters: ccmAdditionalData,
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
    (tagSuffix ?? '') + ' Contract',
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
