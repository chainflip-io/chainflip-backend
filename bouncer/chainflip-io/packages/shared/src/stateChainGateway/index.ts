import type { BigNumber, ContractReceipt, Signer } from 'ethers';
import { checkAllowance, getTokenContractAddress } from '../contracts';
import { Assets, ChainflipNetwork } from '../enums';
import { assert } from '../guards';
import { getStateChainGateway } from './utils';

type WithNonce<T> = T & { nonce?: number | bigint | string };

export type SignerOptions = WithNonce<
  | { network: ChainflipNetwork; signer: Signer }
  | {
      network: 'localnet';
      signer: Signer;
      stateChainGatewayContractAddress: string;
    }
>;

type ExtendLocalnetOptions<T, U> = T extends { network: 'localnet' }
  ? T & U
  : T;

export type FundStateChainAccountOptions = ExtendLocalnetOptions<
  SignerOptions,
  { flipContractAddress: string }
>;

export const fundStateChainAccount = async (
  accountId: `0x${string}`,
  amount: string,
  options: FundStateChainAccountOptions,
): Promise<ContractReceipt> => {
  const flipContractAddress =
    options.network === 'localnet'
      ? options.flipContractAddress
      : getTokenContractAddress(Assets.FLIP, options.network);

  const stateChainGateway = getStateChainGateway(options);

  const { isAllowable } = await checkAllowance(
    amount,
    stateChainGateway.address,
    flipContractAddress,
    options.signer,
  );
  assert(isAllowable, 'Insufficient allowance');

  const transaction = await stateChainGateway.fundStateChainAccount(
    accountId,
    amount,
    { nonce: options.nonce },
  );

  return transaction.wait(1);
};

export const executeRedemption = async (
  accountId: `0x${string}`,
  { nonce, ...options }: WithNonce<SignerOptions>,
): Promise<ContractReceipt> => {
  const stateChainGateway = getStateChainGateway(options);

  const transaction = await stateChainGateway.executeRedemption(accountId, {
    nonce,
  });

  return transaction.wait(1);
};

export const getMinimumFunding = (
  options: SignerOptions,
): Promise<BigNumber> => {
  const stateChainGateway = getStateChainGateway(options);

  return stateChainGateway.getMinimumFunding();
};

export const getRedemptionDelay = (options: SignerOptions): Promise<number> => {
  const stateChainGateway = getStateChainGateway(options);

  return stateChainGateway.REDEMPTION_DELAY();
};

export * from './approval';
