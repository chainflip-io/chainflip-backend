import { Signer, Overrides, ContractTransactionReceipt } from 'ethers';
import { ERC20, ERC20__factory } from './abis';
import { ADDRESSES } from './consts';
import { type ChainflipNetwork, type Asset, Assets } from './enums';

export type TransactionOptions = {
  gasLimit?: bigint;
  gasPrice?: bigint;
  maxFeePerGas?: bigint;
  maxPriorityFeePerGas?: bigint;
  nonce?: number;
  wait?: number;
};

export const extractOverrides = (
  transactionOverrides: TransactionOptions,
): Overrides => {
  const { wait, ...ethersOverrides } = transactionOverrides;

  return ethersOverrides;
};

export function getTokenContractAddress(
  asset: Asset,
  network: ChainflipNetwork,
): string;
export function getTokenContractAddress(
  asset: Asset,
  network: ChainflipNetwork,
  assert: false,
): string | undefined;
export function getTokenContractAddress(
  asset: Asset,
  network: ChainflipNetwork,
  assert = true,
): string | undefined {
  if (asset === Assets.FLIP) return ADDRESSES[network].FLIP_CONTRACT_ADDRESS;
  if (asset === Assets.USDC) return ADDRESSES[network].USDC_CONTRACT_ADDRESS;

  if (assert) {
    throw new Error('Only FLIP and USDC are supported for now');
  }

  return undefined;
}

export const getStateChainGatewayContractAddress = (
  network: ChainflipNetwork,
): string => ADDRESSES[network].STATE_CHAIN_GATEWAY_ADDRESS;

export const checkAllowance = async (
  amount: bigint,
  spenderAddress: string,
  erc20Address: string,
  signer: Signer,
) => {
  const erc20 = ERC20__factory.connect(erc20Address, signer);
  const signerAddress = await signer.getAddress();
  const allowance = await erc20.allowance(signerAddress, spenderAddress);
  return { allowance, isAllowable: allowance >= amount, erc20 };
};

export const approve = async (
  amount: bigint,
  spenderAddress: string,
  erc20: ERC20,
  allowance: bigint,
  txOpts: TransactionOptions,
): Promise<ContractTransactionReceipt | null> => {
  if (allowance >= amount) return null;
  const transaction = await erc20.approve(
    spenderAddress,
    amount - allowance,
    extractOverrides(txOpts),
  );
  return transaction.wait(txOpts.wait);
};

export const getVaultManagerContractAddress = (
  network: ChainflipNetwork,
): string => ADDRESSES[network].VAULT_CONTRACT_ADDRESS;

export const getFlipBalance = async (
  network: ChainflipNetwork,
  signer: Signer,
): Promise<bigint> => {
  const flipAddress = getTokenContractAddress('FLIP', network);
  const flip = ERC20__factory.connect(flipAddress, signer);
  return flip.balanceOf(await signer.getAddress());
};
