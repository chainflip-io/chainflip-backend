import { ContractTransactionReceipt } from 'ethers';
import {
  checkAllowance,
  getStateChainGatewayContractAddress,
  getTokenContractAddress,
  approve,
  TransactionOptions,
} from '../contracts';
import { Assets } from '../enums';
import { FundingNetworkOptions } from './index';

export const checkStateChainGatewayAllowance = async (
  amount: bigint,
  networkOpts: FundingNetworkOptions,
): ReturnType<typeof checkAllowance> => {
  const flipContractAddress =
    networkOpts.network === 'localnet'
      ? networkOpts.flipContractAddress
      : getTokenContractAddress(Assets.FLIP, networkOpts.network);

  const stateChainGatewayContractAddress =
    networkOpts.network === 'localnet'
      ? networkOpts.stateChainGatewayContractAddress
      : getStateChainGatewayContractAddress(networkOpts.network);

  return checkAllowance(
    amount,
    stateChainGatewayContractAddress,
    flipContractAddress,
    networkOpts.signer,
  );
};

export const approveStateChainGateway = async (
  amount: bigint,
  networkOpts: FundingNetworkOptions,
  txOpts: TransactionOptions,
): Promise<ContractTransactionReceipt | null> => {
  const { allowance, erc20, isAllowable } =
    await checkStateChainGatewayAllowance(amount, networkOpts);

  if (isAllowable) return null;

  const stateChainGatewayContractAddress =
    networkOpts.network === 'localnet'
      ? networkOpts.stateChainGatewayContractAddress
      : getStateChainGatewayContractAddress(networkOpts.network);

  return approve(
    amount,
    stateChainGatewayContractAddress,
    erc20,
    allowance,
    txOpts,
  );
};
