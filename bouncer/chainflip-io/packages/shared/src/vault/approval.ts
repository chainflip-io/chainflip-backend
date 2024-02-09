import { ContractTransactionReceipt } from 'ethers';
import { TokenSwapParams } from './schemas';
import {
  checkAllowance,
  getTokenContractAddress,
  getVaultManagerContractAddress,
  approve,
  TransactionOptions,
} from '../contracts';
import { assert } from '../guards';
import { SwapNetworkOptions } from './index';

export const checkVaultAllowance = (
  params: Pick<TokenSwapParams, 'srcAsset' | 'amount'>,
  networkOpts: SwapNetworkOptions,
): ReturnType<typeof checkAllowance> => {
  const erc20Address =
    networkOpts.network === 'localnet'
      ? networkOpts.srcTokenContractAddress
      : getTokenContractAddress(params.srcAsset, networkOpts.network);

  assert(erc20Address !== undefined, 'Missing ERC20 contract address');

  const vaultContractAddress =
    networkOpts.network === 'localnet'
      ? networkOpts.vaultContractAddress
      : getVaultManagerContractAddress(networkOpts.network);

  return checkAllowance(
    BigInt(params.amount),
    vaultContractAddress,
    erc20Address,
    networkOpts.signer,
  );
};

export const approveVault = async (
  params: Pick<TokenSwapParams, 'srcAsset' | 'amount'>,
  networkOpts: SwapNetworkOptions,
  txOpts: TransactionOptions,
): Promise<ContractTransactionReceipt | null> => {
  const { isAllowable, erc20, allowance } = await checkVaultAllowance(
    params,
    networkOpts,
  );

  if (isAllowable) return null;

  const vaultContractAddress =
    networkOpts.network === 'localnet'
      ? networkOpts.vaultContractAddress
      : getVaultManagerContractAddress(networkOpts.network);

  return approve(
    BigInt(params.amount),
    vaultContractAddress,
    erc20,
    allowance,
    txOpts,
  );
};
