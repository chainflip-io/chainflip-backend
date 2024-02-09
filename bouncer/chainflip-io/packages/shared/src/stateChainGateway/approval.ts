import { ContractReceipt } from 'ethers';
import {
  checkAllowance,
  getStateChainGatewayContractAddress,
  getTokenContractAddress,
  approve,
} from '../contracts';
import { Assets } from '../enums';
import type { FundStateChainAccountOptions } from './index';

export const checkStateChainGatewayAllowance = async (
  amount: bigint | string | number,
  options: FundStateChainAccountOptions,
): ReturnType<typeof checkAllowance> => {
  const flipContractAddress =
    options.network === 'localnet'
      ? options.flipContractAddress
      : getTokenContractAddress(Assets.FLIP, options.network);

  const stateChainGatewayContractAddress =
    options.network === 'localnet'
      ? options.stateChainGatewayContractAddress
      : getStateChainGatewayContractAddress(options.network);

  return checkAllowance(
    amount,
    stateChainGatewayContractAddress,
    flipContractAddress,
    options.signer,
  );
};

export const approveStateChainGateway = async (
  amount: bigint | string | number,
  options: FundStateChainAccountOptions,
): Promise<ContractReceipt | null> => {
  const { allowance, erc20, isAllowable } =
    await checkStateChainGatewayAllowance(amount, options);

  if (isAllowable) return null;

  const stateChainGatewayContractAddress =
    options.network === 'localnet'
      ? options.stateChainGatewayContractAddress
      : getStateChainGatewayContractAddress(options.network);

  const receipt = await approve(
    amount,
    stateChainGatewayContractAddress,
    erc20,
    allowance,
    options.nonce,
  );

  return receipt;
};
