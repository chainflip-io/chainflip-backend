import { createTRPCProxyClient, httpBatchLink } from '@trpc/client';
import { Signer } from 'ethers';
import superjson from 'superjson';
import { TransactionOptions } from '@/shared/contracts';
import {
  ChainflipNetwork,
  Chain,
  ChainflipNetworks,
  UncheckedAssetAndChain,
} from '@/shared/enums';
import { assert } from '@/shared/guards';
import {
  ChainAssetMap,
  Environment,
  RpcConfig,
  getEnvironment,
} from '@/shared/rpc';
import { validateSwapAmount } from '@/shared/rpc/utils';
import { ExecuteSwapParams, approveVault, executeSwap } from '@/shared/vault';
import type { TokenSwapParams } from '@/shared/vault/schemas';
import type { AppRouter } from '@/swap/server';
import { BACKEND_SERVICE_URLS } from './consts';
import ApiService, { RequestOptions } from './services/ApiService';
import type {
  ChainData,
  AssetData,
  QuoteRequest,
  DepositAddressResponse,
  QuoteResponse,
  SwapStatusRequest,
  SwapStatusResponse,
  DepositAddressRequest,
} from './types';

type TransactionHash = `0x${string}`;

export type SwapSDKOptions = {
  network?: ChainflipNetwork;
  signer?: Signer;
  backendUrl?: string;
  broker?: {
    url: string;
    commissionBps: number;
  };
  rpcUrl?: string;
};

export class SwapSDK {
  private readonly baseUrl: string;

  private readonly network: ChainflipNetwork;

  private readonly signer?: Signer;

  private readonly trpc;

  private readonly brokerConfig?;

  private stateChainEnvironment?: Environment;

  private readonly rpcConfig: RpcConfig;

  constructor(options: SwapSDKOptions = {}) {
    this.network = options.network ?? ChainflipNetworks.perseverance;
    this.baseUrl = options.backendUrl ?? BACKEND_SERVICE_URLS[this.network];
    this.signer = options.signer;
    this.brokerConfig = options.broker;
    this.trpc = createTRPCProxyClient<AppRouter>({
      transformer: superjson,
      links: [httpBatchLink({ url: new URL('/trpc', this.baseUrl) })],
    });
    this.rpcConfig = options.rpcUrl
      ? { rpcUrl: options.rpcUrl }
      : { network: this.network };
  }

  getChains(sourceChain?: Chain): Promise<ChainData[]> {
    if (sourceChain !== undefined) {
      return ApiService.getPossibleDestinationChains(sourceChain, this.network);
    }
    return ApiService.getChains(this.network);
  }

  private async getStateChainEnvironment(): Promise<Environment> {
    this.stateChainEnvironment ??= await getEnvironment(this.rpcConfig);

    return this.stateChainEnvironment;
  }

  async getAssets(chain: Chain): Promise<AssetData[]> {
    const env = await this.getStateChainEnvironment();

    return ApiService.getAssets(chain, this.network, env);
  }

  getQuote(
    quoteRequest: QuoteRequest,
    options: RequestOptions = {},
  ): Promise<QuoteResponse> {
    return ApiService.getQuote(
      this.baseUrl,
      {
        ...quoteRequest,
        brokerCommissionBps: this.brokerConfig?.commissionBps,
      },
      options,
    );
  }

  async requestDepositAddress(
    depositAddressRequest: DepositAddressRequest,
  ): Promise<DepositAddressResponse> {
    const { srcChain, srcAsset, amount } = depositAddressRequest;

    await this.validateSwapAmount(
      { chain: srcChain, asset: srcAsset },
      BigInt(amount),
    );

    let response;

    if (this.brokerConfig !== undefined) {
      const { requestSwapDepositAddress } = await import('@/shared/broker');

      const result = await requestSwapDepositAddress(
        depositAddressRequest,
        this.brokerConfig,
        this.network,
      );

      response = {
        id: `${result.issuedBlock}-${depositAddressRequest.srcChain}-${result.channelId}`,
        depositAddress: result.address,
        brokerCommissionBps: this.brokerConfig.commissionBps,
        srcChainExpiryBlock: result.sourceChainExpiryBlock,
      };
    } else {
      response = await this.trpc.openSwapDepositChannel.mutate(
        depositAddressRequest,
      );
    }

    return {
      ...depositAddressRequest,
      depositChannelId: response.id,
      depositAddress: response.depositAddress,
      brokerCommissionBps: response.brokerCommissionBps,
      depositChannelExpiryBlock: response.srcChainExpiryBlock as bigint,
      estimatedDepositChannelExpiryTime: response.estimatedExpiryTime,
    };
  }

  getStatus(
    swapStatusRequest: SwapStatusRequest,
    options: RequestOptions = {},
  ): Promise<SwapStatusResponse> {
    return ApiService.getStatus(this.baseUrl, swapStatusRequest, options);
  }

  async executeSwap(
    params: ExecuteSwapParams,
    txOpts: TransactionOptions = {},
  ): Promise<TransactionHash> {
    const { srcChain, srcAsset, amount } = params;

    await this.validateSwapAmount(
      { chain: srcChain, asset: srcAsset },
      BigInt(amount),
    );

    assert(this.signer, 'No signer provided');
    const receipt = await executeSwap(
      params,
      {
        network: this.network,
        signer: this.signer,
      },
      txOpts,
    );
    return receipt.hash as `0x${string}`;
  }

  async approveVault(
    params: Pick<TokenSwapParams, 'srcAsset' | 'amount'>,
    txOpts: TransactionOptions = {},
  ): Promise<TransactionHash | null> {
    if (!('srcAsset' in params)) return null;
    assert(this.signer, 'No signer provided');

    const receipt = await approveVault(
      params,
      {
        signer: this.signer,
        network: this.network,
      },
      txOpts,
    );
    return receipt ? (receipt.hash as `0x${string}`) : null;
  }

  private async validateSwapAmount(
    asset: UncheckedAssetAndChain,
    amount: bigint,
  ): Promise<void> {
    const stateChainEnv = await this.getStateChainEnvironment();

    const result = validateSwapAmount(stateChainEnv, asset, amount);

    if (!result.success) throw new Error(result.reason);
  }

  async getSwapLimits(): Promise<{
    minimumSwapAmounts: ChainAssetMap<bigint>;
    maximumSwapAmounts: ChainAssetMap<bigint | null>;
  }> {
    const {
      swapping: { maximumSwapAmounts },
      ingressEgress: { minimumDepositAmounts },
    } = await this.getStateChainEnvironment();

    return { minimumSwapAmounts: minimumDepositAmounts, maximumSwapAmounts };
  }
}
