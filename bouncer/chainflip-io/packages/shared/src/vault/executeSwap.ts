import { ContractReceipt } from 'ethers';
import { Vault__factory } from '../abis';
import {
  checkAllowance,
  getTokenContractAddress,
  getVaultManagerContractAddress,
} from '../contracts';
import { assetContractIds, chainContractIds } from '../enums';
import { assert, isTokenSwap } from '../guards';
import {
  executeOptionsSchema,
  type ExecuteOptions,
  type ExecuteSwapParams,
  executeSwapParamsSchema,
  type NativeSwapParams,
  type TokenSwapParams,
} from './schemas';

const swapNative = async (
  { destChain, destAsset, destAddress, amount }: NativeSwapParams,
  { nonce, ...opts }: ExecuteOptions,
): Promise<ContractReceipt> => {
  const vaultContractAddress =
    opts.network === 'localnet'
      ? opts.vaultContractAddress
      : getVaultManagerContractAddress(opts.network);

  const vault = Vault__factory.connect(vaultContractAddress, opts.signer);

  const transaction = await vault.xSwapNative(
    chainContractIds[destChain],
    destAddress,
    assetContractIds[destAsset],
    [],
    { value: amount, nonce },
  );

  return transaction.wait(1);
};

const swapToken = async (
  params: TokenSwapParams,
  opts: ExecuteOptions,
): Promise<ContractReceipt> => {
  const vaultContractAddress =
    opts.network === 'localnet'
      ? opts.vaultContractAddress
      : getVaultManagerContractAddress(opts.network);

  const erc20Address =
    opts.network === 'localnet'
      ? opts.srcTokenContractAddress
      : getTokenContractAddress(params.srcAsset, opts.network);

  assert(erc20Address !== undefined, 'Missing ERC20 contract address');

  const { isAllowable } = await checkAllowance(
    params.amount,
    vaultContractAddress,
    erc20Address,
    opts.signer,
  );
  assert(isAllowable, 'Swap amount exceeds allowance');

  const vault = Vault__factory.connect(vaultContractAddress, opts.signer);

  const transaction = await vault.xSwapToken(
    chainContractIds[params.destChain],
    params.destAddress,
    assetContractIds[params.destAsset],
    erc20Address,
    params.amount,
    [],
    { nonce: opts.nonce },
  );

  return transaction.wait(1);
};

const executeSwap = async (
  params: ExecuteSwapParams,
  options: ExecuteOptions,
): Promise<ContractReceipt> => {
  const parsedParams = executeSwapParamsSchema.parse(params);
  const opts = executeOptionsSchema.parse(options);

  return isTokenSwap(parsedParams)
    ? swapToken(parsedParams, opts)
    : swapNative(parsedParams, opts);
};

export default executeSwap;
