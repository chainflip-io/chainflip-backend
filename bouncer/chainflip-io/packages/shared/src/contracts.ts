import { BigNumberish, ContractReceipt, Signer, BigNumber } from 'ethers';
import { ERC20, ERC20__factory } from './abis';
import { ADDRESSES, GOERLI_USDC_CONTRACT_ADDRESS } from './consts';
import {
  type ChainflipNetwork,
  type Asset,
  Assets,
  ChainflipNetworks,
} from './enums';
import { assert } from './guards';

export const getTokenContractAddress = (
  asset: Asset,
  network: ChainflipNetwork,
): string => {
  assert(network !== ChainflipNetworks.mainnet, 'Mainnet is not yet supported');

  if (asset === Assets.FLIP) return ADDRESSES[network].FLIP_CONTRACT_ADDRESS;

  assert(asset === Assets.USDC, 'Only FLIP and USDC are supported for now');

  return GOERLI_USDC_CONTRACT_ADDRESS;
};

export const getStateChainGatewayContractAddress = (
  network: ChainflipNetwork,
): string => {
  assert(network !== ChainflipNetworks.mainnet, 'Mainnet is not yet supported');
  return ADDRESSES[network].STATE_CHAIN_MANAGER_CONTRACT_ADDRESS;
};

export const checkAllowance = async (
  amount: BigNumberish,
  spenderAddress: string,
  erc20Address: string,
  signer: Signer,
) => {
  const erc20 = ERC20__factory.connect(erc20Address, signer);
  const signerAddress = await signer.getAddress();
  const allowance = await erc20.allowance(signerAddress, spenderAddress);
  return { allowance, isAllowable: allowance.gte(amount), erc20 };
};

export const approve = async (
  amount: BigNumberish,
  spenderAddress: string,
  erc20: ERC20,
  allowance: BigNumberish,
  nonce?: bigint | number | string,
): Promise<ContractReceipt | null> => {
  const amountBigNumber = BigNumber.from(amount);
  const allowanceBigNumber = BigNumber.from(allowance);
  if (allowanceBigNumber.gte(amountBigNumber)) return null;
  const requiredAmount = amountBigNumber.sub(allowanceBigNumber);
  const tx = await erc20.approve(spenderAddress, requiredAmount, { nonce });
  return tx.wait(1);
};

export const getVaultManagerContractAddress = (
  network: ChainflipNetwork,
): string => {
  assert(network !== ChainflipNetworks.mainnet, 'Mainnet is not yet supported');
  return ADDRESSES[network].VAULT_CONTRACT_ADDRESS;
};

export const getFlipBalance = async (
  network: ChainflipNetwork,
  signer: Signer,
): Promise<bigint> => {
  const flipAddress = getTokenContractAddress('FLIP', network);
  const flip = ERC20__factory.connect(flipAddress, signer);
  const balance = await flip.balanceOf(await signer.getAddress());
  return balance.toBigInt();
};
