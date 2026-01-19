import { InternalAsset as Asset } from '@chainflip/cli';
import { Keypair, PublicKey } from '@solana/web3.js';
import { u8aToHex } from '@polkadot/util';
import { randomAsHex } from 'polkadot/util-crypto';
import { performSwap, performVaultSwap } from 'shared/perform_swap';
import {
  chainFromAsset,
  getContractAddress,
  solVersionedCcmAdditionalDataCodec,
  newAssetAddress,
} from 'shared/utils';
import { BtcAddressType } from 'shared/new_btc_address';
import { CcmDepositMetadata } from 'shared/new_swap';
import { SwapContext, SwapStatus } from 'shared/utils/swap_context';
import { estimateCcmCfTesterGas } from 'shared/send_evm';
import { Logger } from 'shared/utils/logger';
import { ChainflipIO } from 'shared/utils/chainflip_io';

let swapCount = 1;

// Protocol limits
const MAX_CCM_MSG_LENGTH = 15_000;
const MAX_CCM_ADDITIONAL_DATA_LENGTH = 3000;

// In Arbitrum's localnet large messages (~ >4k) end up with large gas estimations
// of >70M gas, surpassing our hardcoded gas limit (25M) and Arbitrum's block gas
// gas limit (32M). We cap it to a lower value than Ethereum to work around that.
const ARB_MAX_CCM_MSG_LENGTH = MAX_CCM_MSG_LENGTH / 5;

// Solana transactions have a length of 1232. Capping it to some reasonable values
// that when construction the call the Solana length is not exceeded. Technically the
// check should be tx length (dstAsset, srcAsset, ccmData, cf_parameters...) < 1232
const MAX_SOL_VAULT_SWAP_CCM_MESSAGE_LENGTH = 300;
const MAX_SOL_VAULT_SWAP_ADDITIONAL_METADATA_LENGTH = 150;

// Solana CCM-related parameters. These are limits in the protocol downstream from
// Solana's transaction size limits.
const MAX_CCM_BYTES_SOL = 783;
const MAX_CCM_BYTES_USDC = 694;
const SOLANA_BYTES_PER_ACCOUNT = 33;
const BYTES_PER_ALT = 34; // 32 + 1 + 1 (for vector lengths)

function newSolanaCcmAdditionalData(maxBytes: number) {
  // Test all combinations
  const useLegacy = maxBytes < BYTES_PER_ALT || Math.random() < 0.5;
  const useAlt = !useLegacy && Math.random() < 0.5;
  let bytesAvailable = maxBytes;

  const additionalAccounts = [];
  const cfReceiverAddress = getContractAddress('Solana', 'CFTESTER');
  const fallbackAddress = Keypair.generate().publicKey.toBytes();

  if (useAlt) {
    // We will only use one ALT
    bytesAvailable -= BYTES_PER_ALT;
    // We are passing cfTester in the ALT so we have extra bytes available.
    const usedAccountsInAlt = 1;
    bytesAvailable += usedAccountsInAlt * 32 - usedAccountsInAlt * 1;
  }

  const maxAccounts = Math.floor(bytesAvailable / SOLANA_BYTES_PER_ACCOUNT);
  const numAdditionalAccounts = Math.floor(Math.random() * maxAccounts);

  for (let i = 0; i < numAdditionalAccounts; i++) {
    additionalAccounts.push({
      pubkey: Keypair.generate().publicKey.toBytes(),
      is_writable: Math.random() < 0.5,
    });
  }

  bytesAvailable -= numAdditionalAccounts * SOLANA_BYTES_PER_ACCOUNT;

  const ccmAdditionalData = {
    cf_receiver: {
      pubkey: new PublicKey(cfReceiverAddress).toBytes(),
      is_writable: false,
    },
    additional_accounts: additionalAccounts,
    fallback_address: fallbackAddress,
  };

  if (useLegacy) {
    return u8aToHex(
      solVersionedCcmAdditionalDataCodec.enc({
        tag: 'V0',
        value: ccmAdditionalData,
      }),
    );
  }

  const ccmAltAdditionalData = {
    ccm_accounts: ccmAdditionalData,
    alts: useAlt
      ? [new PublicKey(getContractAddress('Solana', 'USER_ADDRESS_LOOKUP_TABLE')).toBytes()]
      : [],
  };

  return u8aToHex(
    solVersionedCcmAdditionalDataCodec.enc({
      tag: 'V1',
      value: ccmAltAdditionalData,
    }),
  );
}

// Generate random bytes. Setting a minimum length of 10 because very short messages can end up
// with the SC returning an ASCII character in SwapDepositAddressReady.
function newCcmArbitraryBytes(maxLength: number): string {
  return randomAsHex(Math.floor(Math.random() * Math.max(0, maxLength - 10)) + 10);
}

// For Solana the maximum number of extra accounts that can be passed is limited by the tx size
// and therefore also depends on the message length.
function newCcmAdditionalData(destAsset: Asset, message: string, maxLength?: number): string {
  const destChain = chainFromAsset(destAsset);

  switch (destChain) {
    case 'Ethereum':
    case 'Arbitrum':
      return '0x';
    case 'Solana': {
      const messageLength = message.slice(2).length / 2;
      let bytesAvailable =
        (destAsset === 'Sol' ? MAX_CCM_BYTES_SOL : MAX_CCM_BYTES_USDC) - messageLength;
      if (maxLength !== undefined) {
        bytesAvailable = Math.min(bytesAvailable, maxLength);
      }
      const ccmAdditionalData = newSolanaCcmAdditionalData(bytesAvailable);
      if (ccmAdditionalData.slice(2).length / 2 > MAX_CCM_ADDITIONAL_DATA_LENGTH) {
        throw new Error(`CCM additional data length exceeds limit: ${ccmAdditionalData.length}`);
      }
      return ccmAdditionalData;
    }
    default:
      throw new Error(`Unsupported chain: ${destChain}`);
  }
}

function newCcmMessage(destAsset: Asset, maxLength?: number): string {
  const destChain = chainFromAsset(destAsset);
  let length: number;

  switch (destChain) {
    case 'Ethereum':
      length = MAX_CCM_MSG_LENGTH;
      break;
    case 'Arbitrum':
      length = ARB_MAX_CCM_MSG_LENGTH;
      break;
    case 'Solana':
      length = destAsset === 'Sol' ? MAX_CCM_BYTES_SOL : MAX_CCM_BYTES_USDC;
      break;
    default:
      throw new Error(`Unsupported chain: ${destChain}`);
  }

  if (maxLength !== undefined) {
    length = Math.min(length, maxLength);
  }

  return newCcmArbitraryBytes(length);
}
// Minimum overhead to ensure simple CCM transactions succeed
const OVERHEAD_COMPUTE_UNITS = 30000;

export async function newCcmMetadata(
  destAsset: Asset,
  ccmMessage?: string,
  ccmAdditionalDataArray?: string,
): Promise<CcmDepositMetadata> {
  const message = ccmMessage ?? newCcmMessage(destAsset);
  const ccmAdditionalData = ccmAdditionalDataArray ?? newCcmAdditionalData(destAsset, message);
  const destChain = chainFromAsset(destAsset);

  let userLogicGasBudget;
  if (destChain === 'Arbitrum' || destChain === 'Ethereum') {
    // Do the gas estimation of the call to the CF Tester contract. CF will then add the extra
    // overhead on top. This is particularly relevant for Arbitrum where estimating the gas here
    // required for execution is very complicated without using `eth_estimateGas` on the user's side.
    // This is what integrators are expected to do and it''ll give a good estimate of the gas
    // needed for the user logic.
    userLogicGasBudget = await estimateCcmCfTesterGas(destChain, message);
  } else if (destChain === 'Solana') {
    // We don't bother estimating in Solana since the gas needed doesn't really change upon the message length.
    userLogicGasBudget = OVERHEAD_COMPUTE_UNITS.toString();
  } else {
    throw new Error(`Unsupported chain: ${destChain}`);
  }

  return {
    message,
    gasBudget: userLogicGasBudget?.toString(),
    ccmAdditionalData,
  };
}

// Vault swaps have some limitations depending on the source chain
export async function newVaultSwapCcmMetadata(
  sourceAsset: Asset,
  destAsset: Asset,
  ccmMessage?: string,
  ccmAdditionalDataArray?: string,
): Promise<CcmDepositMetadata> {
  const sourceChain = chainFromAsset(sourceAsset);
  let messageMaxLength;
  let metadataMaxLength;

  // Solana has restrictions on transaction length
  if (sourceChain === 'Solana') {
    messageMaxLength = MAX_SOL_VAULT_SWAP_CCM_MESSAGE_LENGTH;
    metadataMaxLength = MAX_SOL_VAULT_SWAP_ADDITIONAL_METADATA_LENGTH;
    if (ccmMessage && ccmMessage.length / 2 > messageMaxLength) {
      throw new Error(
        `Message length for Solana vault swap must be less than ${messageMaxLength} bytes`,
      );
    }
    if (ccmAdditionalDataArray && ccmAdditionalDataArray.length / 2 > metadataMaxLength) {
      throw new Error(
        `Additional data length for Solana vault swap must be less than ${metadataMaxLength} bytes`,
      );
    }
  } else if (sourceChain === 'Arbitrum') {
    messageMaxLength = ARB_MAX_CCM_MSG_LENGTH;
    if (ccmMessage && ccmMessage.length / 2 > messageMaxLength) {
      throw new Error(
        `Message length for Solana vault swap must be less than ${messageMaxLength} bytes`,
      );
    }
  }

  const message = ccmMessage ?? newCcmMessage(destAsset, messageMaxLength);
  // For now we only enforce empty ccmAdditionalData for Vault swaps, not deposit channels.
  const ccmAdditionalData =
    ccmAdditionalDataArray ?? newCcmAdditionalData(destAsset, message, metadataMaxLength);
  return newCcmMetadata(destAsset, message, ccmAdditionalData);
}

export async function prepareSwap(
  parentLogger: Logger,
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  tagSuffix?: string,
  swapContext?: SwapContext,
) {
  let tag = `[${(swapCount++).toString().concat(':').padEnd(4, ' ')} ${sourceAsset}->${destAsset}`;
  tag += messageMetadata ? ' CCM' : '';
  tag += tagSuffix ? ` ${tagSuffix}]` : ']';
  const logger = parentLogger.child({ tag });

  const destAddress = await newAssetAddress(
    destAsset,
    undefined,
    addressType,
    messageMetadata !== undefined,
  );
  logger.trace(`${destAsset} address: ${destAddress}`);

  swapContext?.updateStatus(logger, SwapStatus.Initiated);

  return { destAddress, tag };
}

export async function testSwap<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  tagSuffix?: string,
  amount?: string,
) {
  const { destAddress, tag } = await prepareSwap(
    cf.logger,
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    tagSuffix,
    swapContext,
  );

  return performSwap(
    cf.withChildLogger(tag),
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    undefined,
    amount,
    undefined,
    swapContext,
  );
}
export async function testVaultSwap(
  cf: ChainflipIO<[]>,
  sourceAsset: Asset,
  destAsset: Asset,
  addressType?: BtcAddressType,
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  tagSuffix?: string,
) {
  const { destAddress, tag } = await prepareSwap(
    cf.logger,
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    (tagSuffix ?? '') + 'Vault',
    swapContext,
  );

  return performVaultSwap(
    cf.withChildLogger(tag),
    '//BROKER_1',
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    swapContext,
  );
}
