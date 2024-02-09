import { Chain, Asset, chainAssets } from '@/shared/enums';
import { CcmMetadata, QuoteQueryResponse, SwapFee } from '@/shared/schemas';

export interface ChainData {
  id: Chain;
  name: string;
  isMainnet: boolean;
}

export type AssetData = {
  [K in keyof typeof chainAssets]: {
    id: (typeof chainAssets)[K][number];
    chain: K;
    contractAddress: string | undefined;
    decimals: number;
    name: string;
    symbol: string;
    isMainnet: boolean;
    minimumSwapAmount: string;
    maximumSwapAmount: string | null;
    minimumEgressAmount: string;
  };
}[keyof typeof chainAssets];

interface ChainsAndAssets {
  srcChain: Chain;
  destChain: Chain;
  srcAsset: Asset;
  destAsset: Asset;
}

export interface QuoteRequest extends ChainsAndAssets {
  amount: string;
}

export interface QuoteResponse extends QuoteRequest {
  quote: QuoteQueryResponse;
}

export interface DepositAddressRequest extends QuoteRequest {
  destAddress: string;
  ccmMetadata?: CcmMetadata;
}

export interface DepositAddressResponse extends DepositAddressRequest {
  depositChannelId: string;
  depositAddress: string;
  brokerCommissionBps: number;
  depositChannelExpiryBlock: bigint;
  estimatedDepositChannelExpiryTime: number | undefined;
}

export interface SwapStatusRequest {
  id: string;
}

export interface CommonStatusFields extends ChainsAndAssets {
  destAddress: string;
  depositAddress: string | undefined;
  depositChannelCreatedAt: number | undefined;
  depositChannelBrokerCommissionBps: number | undefined;
  expectedDepositAmount: string | undefined;
  depositChannelExpiryBlock: bigint;
  estimatedDepositChannelExpiryTime: number | undefined;
  isDepositChannelExpired: boolean;
  depositChannelOpenedThroughBackend: boolean;
  ccmDepositReceivedBlockIndex: string | undefined;
  ccmMetadata:
    | {
        gasBudget: string;
        message: `0x${string}`;
      }
    | undefined;
  feesPaid: SwapFee[];
}

export type SwapStatusResponse = CommonStatusFields &
  (
    | {
        state: 'AWAITING_DEPOSIT';
        depositAmount: string | undefined;
        depositTransactionHash: string | undefined;
        depositTransactionConfirmations: number | undefined;
      }
    | {
        state: 'DEPOSIT_RECEIVED';
        swapId: string;
        depositAmount: string;
        depositReceivedAt: number;
        depositReceivedBlockIndex: string;
      }
    | {
        state: 'SWAP_EXECUTED';
        swapId: string;
        depositAmount: string;
        depositReceivedAt: number;
        depositReceivedBlockIndex: string;
        intermediateAmount: string | undefined;
        swapExecutedAt: number;
        swapExecutedBlockIndex: string;
      }
    | {
        state: 'EGRESS_SCHEDULED';
        swapId: string;
        depositAmount: string;
        depositReceivedAt: number;
        depositReceivedBlockIndex: string;
        intermediateAmount: string | undefined;
        swapExecutedAt: number;
        swapExecutedBlockIndex: string;
        egressAmount: string;
        egressScheduledAt: number;
        egressScheduledBlockIndex: string;
      }
    | {
        state: 'BROADCAST_REQUESTED' | 'BROADCASTED';
        swapId: string;
        depositAmount: string;
        depositReceivedAt: number;
        depositReceivedBlockIndex: string;
        intermediateAmount: string | undefined;
        swapExecutedAt: number;
        swapExecutedBlockIndex: string;
        egressAmount: string;
        egressScheduledAt: number;
        egressScheduledBlockIndex: string;
        broadcastRequestedAt: number;
        broadcastRequestedBlockIndex: string;
      }
    | {
        state: 'BROADCAST_ABORTED';
        swapId: string;
        depositAmount: string;
        depositReceivedAt: number;
        depositReceivedBlockIndex: string;
        intermediateAmount: string | undefined;
        swapExecutedAt: number;
        swapExecutedBlockIndex: string;
        egressAmount: string;
        egressScheduledAt: number;
        egressScheduledBlockIndex: string;
        broadcastRequestedAt: number;
        broadcastRequestedBlockIndex: string;
        broadcastAbortedAt: number;
        broadcastAbortedBlockIndex: string;
      }
    | {
        state: 'COMPLETE';
        swapId: string;
        depositAmount: string;
        depositReceivedAt: number;
        depositReceivedBlockIndex: string;
        intermediateAmount: string | undefined;
        swapExecutedAt: number;
        swapExecutedBlockIndex: string;
        egressAmount: string;
        egressScheduledAt: number;
        egressScheduledBlockIndex: string;
        broadcastRequestedAt: number;
        broadcastRequestedBlockIndex: string;
        broadcastSucceededAt: number;
        broadcastSucceededBlockIndex: string;
      }
  );
