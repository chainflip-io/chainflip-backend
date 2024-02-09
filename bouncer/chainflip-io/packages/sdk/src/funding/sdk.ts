import type { Signer } from 'ethers';
import { getFlipBalance, TransactionOptions } from '@/shared/contracts';
import { ChainflipNetwork, ChainflipNetworks } from '@/shared/enums';
import { getFundingEnvironment, RpcConfig } from '@/shared/rpc';
import {
  approveStateChainGateway,
  executeRedemption,
  fundStateChainAccount,
  getMinimumFunding,
  getPendingRedemption,
  getRedemptionDelay,
  PendingRedemption,
} from '@/shared/stateChainGateway';

export type FundingSDKOption = {
  network?: ChainflipNetwork;
  signer: Signer;
  rpcUrl?: string;
};

export type TransactionHash = `0x${string}`;

export class FundingSDK {
  private readonly options: Required<Omit<FundingSDKOption, 'rpcUrl'>>;

  private readonly rpcConfig: RpcConfig;

  private redemptionTax?: bigint;

  constructor(options: FundingSDKOption) {
    const network = options.network ?? ChainflipNetworks.perseverance;
    this.options = {
      signer: options.signer,
      network,
    };
    this.rpcConfig = options.rpcUrl ? { rpcUrl: options.rpcUrl } : { network };
  }

  /**
   * @param accountId the hex-encoded validator account id
   * @param amount the amount to fund in base units of FLIP
   */
  async fundStateChainAccount(
    accountId: `0x${string}`,
    amount: bigint,
    txOpts: TransactionOptions = {},
  ): Promise<TransactionHash> {
    const tx = await fundStateChainAccount(
      accountId,
      amount,
      this.options,
      txOpts,
    );
    return tx.hash as `0x${string}`;
  }

  /**
   * @param accountId the hex-encoded validator account id
   */
  async executeRedemption(
    accountId: `0x${string}`,
    txOpts: TransactionOptions = {},
  ): Promise<TransactionHash> {
    const tx = await executeRedemption(accountId, this.options, txOpts);
    return tx.hash as `0x${string}`;
  }

  async getMinimumFunding(): Promise<bigint> {
    return getMinimumFunding(this.options);
  }

  async getRedemptionDelay(): Promise<bigint> {
    return getRedemptionDelay(this.options);
  }

  async getFlipBalance(): Promise<bigint> {
    return getFlipBalance(this.options.network, this.options.signer);
  }

  async getPendingRedemption(
    accountId: `0x${string}`,
  ): Promise<PendingRedemption | undefined> {
    return getPendingRedemption(accountId, this.options);
  }

  /**
   * @param the amount of FLIP to request approval for
   * @returns the transaction hash or null if no approval was required
   */
  async approveStateChainGateway(
    amount: bigint,
    txOpts: TransactionOptions = {},
  ): Promise<TransactionHash | null> {
    const receipt = await approveStateChainGateway(
      amount,
      this.options,
      txOpts,
    );

    return receipt ? (receipt.hash as `0x${string}`) : null;
  }

  async getRedemptionTax(): Promise<bigint> {
    this.redemptionTax ??= (
      await getFundingEnvironment(this.rpcConfig)
    ).redemptionTax;

    return this.redemptionTax;
  }
}
