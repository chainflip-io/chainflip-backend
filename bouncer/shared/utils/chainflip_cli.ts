import { ethers, Signer, ContractTransactionResponse, Overrides } from 'ethers';
import {
  getErc20abi,
  getStateChainGatewayAbi,
} from 'shared/contract_interfaces';

export type TransactionOptions = {
  gasLimit?: bigint;
  gasPrice?: bigint;
  maxFeePerGas?: bigint;
  maxPriorityFeePerGas?: bigint;
  nonce?: number;
  wait?: number;
};

export type FundingNetworkOptions = {
  network: 'localnet';
  signer: Signer;
  stateChainGatewayContractAddress: string;
  flipContractAddress: string;
};

const extractOverrides = ({ wait: _wait, ...overrides }: TransactionOptions): Overrides =>
  overrides;

const getStateChainGateway = async ({
  stateChainGatewayContractAddress,
  signer,
}: FundingNetworkOptions) => {
  const abi = await getStateChainGatewayAbi();
  return new ethers.Contract(stateChainGatewayContractAddress, abi, signer);
};

export const fundStateChainAccount = async (
  accountId: `0x${string}`,
  amount: bigint,
  networkOpts: FundingNetworkOptions,
  txOpts: TransactionOptions,
): Promise<ContractTransactionResponse> => {
  const stateChainGateway = await getStateChainGateway(networkOpts);

  const erc20Abi = await getErc20abi();
  const flip = new ethers.Contract(networkOpts.flipContractAddress, erc20Abi, networkOpts.signer);
  const signerAddress = await networkOpts.signer.getAddress();
  const allowance = await flip.allowance(signerAddress, await stateChainGateway.getAddress());
  if (allowance < amount) throw new Error('Insufficient allowance');

  const transaction = (await stateChainGateway.fundStateChainAccount(
    accountId,
    amount,
    extractOverrides(txOpts),
  )) as ContractTransactionResponse;
  await transaction.wait(txOpts.wait);

  return transaction;
};

export const executeRedemption = async (
  accountId: `0x${string}`,
  networkOpts: FundingNetworkOptions,
  txOpts: TransactionOptions,
): Promise<ContractTransactionResponse> => {
  const stateChainGateway = await getStateChainGateway(networkOpts);

  const transaction = (await stateChainGateway.executeRedemption(
    accountId,
    extractOverrides(txOpts),
  )) as ContractTransactionResponse;
  await transaction.wait(txOpts.wait);

  return transaction;
};

export const getRedemptionDelay = async (networkOpts: FundingNetworkOptions): Promise<bigint> => {
  const stateChainGateway = await getStateChainGateway(networkOpts);

  return stateChainGateway.REDEMPTION_DELAY();
};
