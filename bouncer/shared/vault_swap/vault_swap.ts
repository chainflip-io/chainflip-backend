import assert from 'assert';
import { ApiPromise } from '@polkadot/api';
import {
  stateChainAssetFromAsset,
  chainFromAsset,
  decodeDotAddressForContract,
  Chains,
  Asset,
} from 'shared/utils';
import { CcmDepositMetadata, DcaParams } from 'shared/new_swap';
import { AssetAndChain } from '@chainflip/utils/chainflip';

function toCcmRpcParams(metadata: CcmDepositMetadata) {
  return {
    message: metadata.message,
    gas_budget: `0x${BigInt(metadata.gasBudget).toString(16)}`,
    ccm_additional_data: metadata.ccmAdditionalData,
  };
}

function toDcaRpcParams(dcaParams: DcaParams) {
  return {
    number_of_chunks: dcaParams.numberOfChunks,
    chunk_interval: dcaParams.chunkIntervalBlocks,
  };
}

type VaultSwapInputRpc = {
  source_asset: AssetAndChain;
  destination_asset: AssetAndChain;
  destination_address: string;
  broker_commission: number;
  boost_fee: number;
  channel_metadata: {
    message: string;
    gas_budget: string;
    ccm_additional_data: string | null;
  } | null;
  affiliate_fees: { account: string; bps: number }[];
  dca_parameters: { number_of_chunks: number; chunk_interval: number } | null;
};

const evmChains: ReadonlySet<string> = new Set([Chains.Ethereum, Chains.Arbitrum]);

/**
 * Requests the encoded vault swap parameters using the `cf_request_swap_parameter_encoding` RPC.
 * For non-EVM source chains, the encoded result is immediately decoded via the `cf_decode_vault_swap_parameter` RPC as a sanity check.
 */
export async function requestSwapParameterEncoding<T>(
  chainflip: ApiPromise,
  brokerAddress: string,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  brokerCommissionBps: number,
  extraParameters: unknown,
  messageMetadata: CcmDepositMetadata | undefined,
  boostFeeBps: number,
  affiliateFees: { account: string; bps: number }[],
  dcaParams: DcaParams | undefined,
): Promise<T> {
  const encodedDestAddress =
    chainFromAsset(destAsset) === Chains.Assethub
      ? decodeDotAddressForContract(destAddress)
      : destAddress;

  // Encode the payload
  const encoded = (await chainflip.rpc(
    'cf_request_swap_parameter_encoding',
    brokerAddress,
    stateChainAssetFromAsset(sourceAsset),
    stateChainAssetFromAsset(destAsset),
    encodedDestAddress,
    brokerCommissionBps,
    extraParameters,
    messageMetadata ? toCcmRpcParams(messageMetadata) : null,
    boostFeeBps,
    affiliateFees,
    dcaParams ? toDcaRpcParams(dcaParams) : null,
  )) as unknown as T;

  // Sanity check the encoding by decoding it (EVM decoding is not supported)
  if (!evmChains.has(chainFromAsset(sourceAsset))) {
    const decoded = (await chainflip.rpc(
      'cf_decode_vault_swap_parameter',
      brokerAddress,
      encoded,
    )) as unknown as VaultSwapInputRpc;

    // Compare the decoded values with the original input
    assert.deepStrictEqual(decoded.source_asset, stateChainAssetFromAsset(sourceAsset));
    assert.deepStrictEqual(decoded.destination_asset, stateChainAssetFromAsset(destAsset));
    assert.strictEqual(decoded.destination_address.toLowerCase(), encodedDestAddress.toLowerCase());
    assert.strictEqual(decoded.broker_commission, brokerCommissionBps);
    assert.strictEqual(decoded.boost_fee, boostFeeBps);

    if (messageMetadata) {
      assert.strictEqual(
        decoded.channel_metadata?.message.toLowerCase(),
        messageMetadata.message.toLowerCase(),
      );
      assert.strictEqual(
        decoded.channel_metadata?.gas_budget.toLowerCase(),
        `0x${BigInt(messageMetadata.gasBudget).toString(16)}`.toLowerCase(),
      );
      assert.strictEqual(
        decoded.channel_metadata?.ccm_additional_data?.toLowerCase(),
        messageMetadata.ccmAdditionalData?.toLowerCase() ?? null,
      );
    } else {
      assert.strictEqual(decoded.channel_metadata, null);
    }

    assert.strictEqual(decoded.affiliate_fees.length, affiliateFees.length);
    for (let i = 0; i < affiliateFees.length; i++) {
      assert.strictEqual(
        decoded.affiliate_fees[i].account.toLowerCase(),
        affiliateFees[i].account.toLowerCase(),
      );
      assert.strictEqual(decoded.affiliate_fees[i].bps, affiliateFees[i].bps);
    }

    if (dcaParams) {
      assert.strictEqual(decoded.dca_parameters?.number_of_chunks, dcaParams.numberOfChunks);
      assert.strictEqual(decoded.dca_parameters?.chunk_interval, dcaParams.chunkIntervalBlocks);
    } else if (chainFromAsset(sourceAsset) !== Chains.Bitcoin) {
      // BTC always encodes dca_parameters even when not provided
      assert.strictEqual(decoded.dca_parameters, null);
    }
  }

  return encoded;
}
