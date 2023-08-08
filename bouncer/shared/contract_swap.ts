import {
  Asset,
  executeSwap,
  executeCall,
  ExecuteCallParams,
  ExecuteSwapParams,
  approveVault,
  assetChains,
  assetDecimals,
} from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import {
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  getEthContractAddress,
  observeCcmReceived,
  amountToFineAmount,
  defaultAssetAmounts,
} from './utils';
import { getNextEthNonce } from './send_eth';
import { getBalance } from './get_balance';
import { CcmDepositMetadata } from '../shared/new_swap';

export async function executeContractSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
): ReturnType<typeof executeSwap> {
  const wallet = Wallet.fromMnemonic(
    process.env.ETH_USDC_WHALE_MNEMONIC ??
      'test test test test test test test test test test test junk',
  ).connect(getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

  const destChain = assetChains[destAsset];

  const nonce = await getNextEthNonce();
  const options = {
    signer: wallet,
    nonce,
    network: 'localnet',
    vaultContractAddress: getEthContractAddress('VAULT'),
    ...(srcAsset !== 'ETH' ? { srcTokenContractAddress: getEthContractAddress(srcAsset) } : {}),
    gasLimit: 200000,
  } as const;

  const params = {
    destChain,
    destAsset,
    // It is important that this is large enough to result in
    // an amount larger than existential (e.g. on Polkadot):
    amount: amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals[srcAsset]),
    destAddress,
    srcAsset,
    srcChain: assetChains[srcAsset],
  } as ExecuteSwapParams;

  let receipt;
  if (!messageMetadata) {
    receipt = await executeSwap(params, options);
  } else {
    receipt = await executeCall(
      {
        ...params,
        gasBudget: messageMetadata.gasBudget.toString(),
        message: messageMetadata.message,
      } as ExecuteCallParams,
      options,
    );
  }

  return receipt;
}
export type ContractSwapParams = {
  sourceAsset: Asset;
  destAsset: Asset;
  destAddress: string;
  txHash: string;
};

export async function performSwapViaContract(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  swapTag?: string,
  messageMetadata?: CcmDepositMetadata,
): Promise<ContractSwapParams> {
  const api = await getChainflipApi();

  const tag = swapTag ?? '';

  try {
    const oldBalance = await getBalance(destAsset, destAddress);
    console.log(`${tag} Old balance: ${oldBalance}`);
    console.log(
      `${tag} Executing (${sourceAsset}) contract swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
    );
    // To uniquely identify the contractSwap, we need to use the TX hash. This is only known
    // after sending the transaction, so we send it first and observe the events afterwards.
    // There are still multiple blocks of safety margin inbetween before the event is emitted
    const receipt = await executeContractSwap(sourceAsset, destAsset, destAddress, messageMetadata);
    await observeEvent('swapping:SwapScheduled', api, (event) => {
      if ('Vault' in event.data.origin) {
        return event.data.origin.Vault.txHash === receipt.transactionHash;
      }
      // Otherwise it was a swap scheduled by requesting a deposit address
      return false;
    });
    console.log(`${tag} Successfully observed event: swapping: SwapScheduled`);

    const ccmEventEmitted = messageMetadata
      ? observeCcmReceived(sourceAsset, destAsset, destAddress, messageMetadata)
      : Promise.resolve();

    const [newBalance] = await Promise.all([
      observeBalanceIncrease(destAsset, destAddress, oldBalance),
      ccmEventEmitted,
    ]);
    console.log(`${tag} Swap success! New balance: ${newBalance}!`);
    return {
      sourceAsset,
      destAsset,
      destAddress,
      txHash: receipt.transactionHash,
    };
  } catch (err) {
    throw new Error(`${tag} ${err}`);
  }
}

export async function approveTokenVault(srcAsset: 'FLIP' | 'USDC', amount: string) {
  const wallet = Wallet.fromMnemonic(
    process.env.ETH_USDC_WHALE_MNEMONIC ??
      'test test test test test test test test test test test junk',
  ).connect(getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

  await getNextEthNonce((nextNonce) =>
    approveVault(
      {
        amount,
        srcAsset,
      },
      {
        signer: wallet,
        nonce: nextNonce,
        network: 'localnet',
        vaultContractAddress: getEthContractAddress('VAULT'),
        srcTokenContractAddress: getEthContractAddress(srcAsset),
      },
    ),
  );
}
