import { InternalAsset as Asset } from '@chainflip/cli';
import { Keypair, PublicKey } from '@solana/web3.js';
import { u8aToHex } from '@polkadot/util';
import { randomAsHex } from '../polkadot/util-crypto';
import { performSwap, performVaultSwap } from '../shared/perform_swap';
import {
  newAddress,
  chainFromAsset,
  getContractAddress,
  ccmSupportedChains,
  solVersionedCcmAdditionalDataCodec,
} from '../shared/utils';
import { BtcAddressType } from '../shared/new_btc_address';
import { CcmDepositMetadata } from '../shared/new_swap';
import { SwapContext, SwapStatus } from './swap_context';
import { estimateCcmCfTesterGas } from './send_evm';

let swapCount = 1;

export function newSolanaCcmAdditionalData(maxAccounts: number) {
  const cfReceiverAddress = getContractAddress('Solana', 'CFTESTER');

  const fallbackAddress = Keypair.generate().publicKey.toBytes();

  const additionalAccounts = [];
  const numAdditionalAccounts = Math.floor(Math.random() * maxAccounts);

  for (let i = 0; i < numAdditionalAccounts; i++) {
    additionalAccounts.push({
      pubkey: Keypair.generate().publicKey.toBytes(),
      is_writable: Math.random() < 0.5,
    });
  }

  const ccmAdditionalData = {
    cf_receiver: {
      pubkey: new PublicKey(cfReceiverAddress).toBytes(),
      is_writable: false,
    },
    additional_accounts: additionalAccounts,
    fallback_address: fallbackAddress,
  };

  return u8aToHex(
    solVersionedCcmAdditionalDataCodec.enc({
      tag: 'V0',
      value: ccmAdditionalData,
    }),
  );
}

// Generate random bytes. Setting a minimum length of 10 because very short messages can end up
// with the SC returning an ASCII character in SwapDepositAddressReady.
function newCcmArbitraryBytes(maxLength: number): string {
  return randomAsHex(Math.floor(Math.random() * Math.max(0, maxLength - 10)) + 10);
}

// Protocol limits
const MAX_CCM_MSG_LENGTH = 15_000;
const MAX_CCM_ADDITIONAL_DATA_LENGTH = 1000;

// In Arbitrum's localnet large messages (~ >4k) end up with large gas estimations
// of >70M gas, surpassing our hardcoded gas limit (25M) and Arbitrum's block gas
// gas limit (32M). We cap it to a lower value than Ethereum to work around that.
const ARB_MAX_CCM_MSG_LENGTH = MAX_CCM_MSG_LENGTH / 5;

// Solana transactions have a length of 1232. Capping it to some reasonable values
// that when construction the call the Solana length is not exceeded. Technically the
// check should be tx lenght (dstAsset, srcAsset, ccmData, cf_parameters...) < 1232
const MAX_SOL_VAULT_SWAP_CCM_MESSAGE_LENGTH = 300;
const MAX_SOL_VAULT_SWAP_ADDITIONAL_METADATA_LENGTH = 150;

// Solana CCM-related parameters. These are limits in the protocol.
const MAX_CCM_BYTES_SOL = 694 + 32; // Adding 32 for now to account for the empty source_address
const MAX_CCM_BYTES_USDC = 481 + 32; // Adding 32 for now to account for the empty source_address
const SOLANA_BYTES_PER_ACCOUNT = 33;

function newCcmAdditionalData(destAsset: Asset, message?: string, maxLength?: number): string {
  const destChain = chainFromAsset(destAsset);
  let length: number;

  switch (destChain) {
    case 'Ethereum':
    case 'Arbitrum':
      return '0x';
    case 'Solana': {
      const messageLength = (message!.length - 2) / 2;
      length = (destAsset === 'Sol' ? MAX_CCM_BYTES_SOL : MAX_CCM_BYTES_USDC) - messageLength;
      if (maxLength !== undefined) {
        length = Math.min(length, maxLength);
      }
      const maxAccounts = Math.floor(length / SOLANA_BYTES_PER_ACCOUNT);

      // The maximum number of extra accounts that can be passed is limited by the tx size
      // and therefore also depends on the message length.
      const ccmAdditonalData = newSolanaCcmAdditionalData(maxAccounts);
      if (ccmAdditonalData.slice(2).length / 2 > MAX_CCM_ADDITIONAL_DATA_LENGTH) {
        throw new Error(`CCM additional data length exceeds limit: ${ccmAdditonalData.length}`);
      }
      return ccmAdditonalData;
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
const OVERHEAD_COMPUTE_UNITS = 20000;

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
    // Do the gas estimation of the call to the CF Tester contract and add a small buffer.
    // A 5-10% works for almost all swaps. However, there is some flakiness in Arbitrum for
    // swaps that require small amounts of gas. That is because Arbitrum's fees in localnet
    // are extremely high (two orders of magniture larger than in mainnnet) so small gas
    // fluctuations cause flakiness for those CCM swaps with low gas.
    // Therefore we apply a minimum gas budget to ensure the swap succeeds.
    userLogicGasBudget = Math.trunc((await estimateCcmCfTesterGas(message)) * 1.1);
    if (destChain === 'Arbitrum') {
      const minGasBudget = 30000;
      userLogicGasBudget = userLogicGasBudget < minGasBudget ? minGasBudget : userLogicGasBudget;
    }
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
    if (ccmMessage && ccmMessage.slice(2).length / 2 > messageMaxLength) {
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
    if (ccmMessage && ccmMessage.slice(2).length / 2 > messageMaxLength) {
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
export async function testVaultSwap(
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
    (tagSuffix ?? '') + 'Vault',
    log,
    swapContext,
  );

  return performVaultSwap(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
    swapContext,
    log,
  );
}
