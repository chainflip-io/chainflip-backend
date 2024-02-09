import type { Signer } from 'ethers';
import { getFlipBalance } from '@/shared/contracts';
import { ChainflipNetwork, ChainflipNetworks } from '@/shared/enums';
import {
  approveStateChainGateway,
  executeRedemption,
  fundStateChainAccount,
  getMinimumFunding,
  getRedemptionDelay,
} from '@/shared/stateChainGateway';

type SDKOptions = {
  network?: Exclude<ChainflipNetwork, 'mainnet'>;
  signer: Signer;
};

type TransactionHash = string;

export class FundingSDK {
  private readonly options: Required<SDKOptions>;

  constructor(options: SDKOptions) {
    this.options = {
      signer: options.signer,
      network: options.network ?? ChainflipNetworks.perseverance,
    };
  }

  /**
   * @param accountId the hex-encoded validator account id
   * @param amount the amount to fund in base units of FLIP
   * @param signer a signer to use for the transaction if different from the one
   *               provided in the constructor
   */
  async fundStateChainAccount(
    accountId: `0x${string}`,
    amount: string,
  ): Promise<TransactionHash> {
    const tx = await fundStateChainAccount(accountId, amount, this.options);
    return tx.transactionHash;
  }

  /**
   * @param accountId the hex-encoded validator account id
   * @param signer a signer to use for the transaction if different from the one
   *               provided in the constructor
   */
  async executeRedemption(accountId: `0x${string}`): Promise<TransactionHash> {
    const tx = await executeRedemption(accountId, this.options);
    return tx.transactionHash;
  }

  async getMinimumFunding(): Promise<bigint> {
    const amount = await getMinimumFunding(this.options);
    return amount.toBigInt();
  }

  async getRedemptionDelay(): Promise<number> {
    return getRedemptionDelay(this.options);
  }

  async getFlipBalance(): Promise<bigint> {
    return getFlipBalance(this.options.network, this.options.signer);
  }

  /**
   * @param amount the amount of FLIP to request approval for
   * @returns the transaction hash or null if no approval was required
   */
  async approveStateChainGateway(
    amount: bigint | string | number,
    nonce?: bigint | string | number,
  ): Promise<TransactionHash | null> {
    const receipt = await approveStateChainGateway(amount, {
      nonce,
      ...this.options,
    });

    return receipt && receipt.transactionHash;
  }
}
