import { ChainflipAsset as Asset } from '@chainflip/utils/chainflip';
import { stateChainAssetFromAsset } from 'shared/utils';
import { brokerApiRpc } from 'shared/json_rpc';
import { Logger } from 'shared/utils/logger';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';

export type SwapDepositAddressResult = {
  issued_block: number;
  channel_id: number;
  deposit_address: string;
  source_chain_expiry_block: number;
  channel_opening_fee: string;
};

function toCcmRpcParams(metadata: CcmDepositMetadata) {
  return {
    message: metadata.message,
    gas_budget: `0x${BigInt(metadata.gasBudget).toString(16)}`,
    ccm_additional_data: metadata.ccmAdditionalData,
  };
}

function toFokRpcParams(fokParams: FillOrKillParamsX128) {
  return {
    retry_duration: fokParams.retryDurationBlocks,
    refund_address: fokParams.refundAddress,
    min_price: `0x${BigInt(fokParams.minPriceX128).toString(16)}`,
    max_oracle_price_slippage: fokParams.maxOraclePriceSlippage ?? null,
    refund_ccm_metadata: fokParams.refundCcmMetadata
      ? toCcmRpcParams(fokParams.refundCcmMetadata)
      : null,
  };
}

function toDcaRpcParams(dcaParams: DcaParams) {
  return {
    number_of_chunks: dcaParams.numberOfChunks,
    chunk_interval: dcaParams.chunkIntervalBlocks,
  };
}

export async function brokerEncodeCfParameters(
  logger: Logger,
  sourceAsset: Asset,
  destAsset: Asset,
  destinationAddress: string,
  brokerCommissionBps: number,
  fokParams: FillOrKillParamsX128,
  boostFeeBps?: number,
  messageMetadata?: CcmDepositMetadata,
  dcaParams?: DcaParams,
): Promise<unknown> {
  return brokerApiRpc(logger, 'broker_encode_cf_parameters', [
    stateChainAssetFromAsset(sourceAsset),
    stateChainAssetFromAsset(destAsset),
    destinationAddress,
    brokerCommissionBps,
    toFokRpcParams(fokParams),
    messageMetadata ? toCcmRpcParams(messageMetadata) : null,
    boostFeeBps ?? null,
    null, // affiliates
    dcaParams ? toDcaRpcParams(dcaParams) : null,
  ]);
}

export async function brokerRequestSwapDepositAddress(
  logger: Logger,
  sourceAsset: Asset,
  destAsset: Asset,
  destinationAddress: string,
  brokerCommissionBps: number,
  boostFeeBps: number,
  fokParams: FillOrKillParamsX128,
  messageMetadata?: CcmDepositMetadata,
  dcaParams?: DcaParams,
): Promise<SwapDepositAddressResult> {
  return brokerApiRpc(logger, 'broker_request_swap_deposit_address', [
    stateChainAssetFromAsset(sourceAsset),
    stateChainAssetFromAsset(destAsset),
    destinationAddress,
    brokerCommissionBps,
    messageMetadata ? toCcmRpcParams(messageMetadata) : null,
    boostFeeBps,
    null,
    toFokRpcParams(fokParams),
    dcaParams ? toDcaRpcParams(dcaParams) : null,
  ]);
}
