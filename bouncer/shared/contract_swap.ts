import {
  Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  assetDecimals,
} from '@chainflip-io/cli';
import { Wallet, getDefaultProvider } from 'ethers';
import {
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  getEvmContractAddress,
  observeCcmReceived,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  getEvmEndpoint,
  getWhaleMnemonic,
} from './utils';
import { getNextEvmNonce } from './send_evm';
import { getBalance } from './get_balance';
import { CcmDepositMetadata } from '../shared/new_swap';

export async function executeContractSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
): ReturnType<typeof executeSwap> {
  const srcChain = chainFromAsset(srcAsset);
  const wallet = Wallet.fromPhrase(getWhaleMnemonic(srcChain)).connect(
    getDefaultProvider(getEvmEndpoint(srcChain)),
  );

  const destChain = chainFromAsset(destAsset);

  const nonce = await getNextEvmNonce(srcChain);
  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    vaultContractAddress: getEvmContractAddress(srcChain, 'VAULT'),
    srcTokenContractAddress: getEvmContractAddress(srcChain, srcAsset),
  } as const;
  const txOptions = {
    nonce,
    gasLimit: 200000n,
  } as const;

  const receipt = await executeSwap(
    {
      destChain,
      destAsset,
      // It is important that this is large enough to result in
      // an amount larger than existential (e.g. on Polkadot):
      amount: amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals[srcAsset]),
      destAddress,
      srcAsset,
      srcChain,
      ccmMetadata: messageMetadata && {
        gasBudget: messageMetadata.gasBudget.toString(),
        message: messageMetadata.message,
      },
    } as ExecuteSwapParams,
    networkOptions,
    txOptions,
  );

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
        const sourceAssetMatches = sourceAsset === (event.data.sourceAsset.toUpperCase() as Asset);
        const destAssetMatches = destAsset === (event.data.destinationAsset.toUpperCase() as Asset);
        const txHashMatches = event.data.origin.Vault.txHash === receipt.hash;
        return sourceAssetMatches && destAssetMatches && txHashMatches;
      }
      // Otherwise it was a swap scheduled by requesting a deposit address
      return false;
    });
    console.log(`${tag} Successfully observed event: swapping: SwapScheduled`);

    const ccmEventEmitted = messageMetadata
      ? observeCcmReceived(
          sourceAsset,
          destAsset,
          destAddress,
          messageMetadata,
          Wallet.fromPhrase(getWhaleMnemonic(chainFromAsset(sourceAsset))).address.toLowerCase(),
        )
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
      txHash: receipt.hash,
    };
  } catch (err) {
    console.error('err:', err);
    if (err instanceof Error) {
      console.log(err.stack);
    }
    throw new Error(`${tag} ${err}`);
  }
}

export async function approveTokenVault(srcAsset: 'FLIP' | 'USDC' | 'ARBUSDC', amount: string) {
  const chain = chainFromAsset(srcAsset as Asset);

  const wallet = Wallet.fromPhrase(getWhaleMnemonic(chain)).connect(
    getDefaultProvider(getEvmEndpoint(chain)),
  );

  await getNextEvmNonce(chain, (nextNonce) =>
    approveVault(
      {
        amount,
        srcAsset,
      },
      {
        signer: wallet,
        network: 'localnet',
        vaultContractAddress: getEvmContractAddress(chain, 'VAULT'),
        srcTokenContractAddress: getEvmContractAddress(chain, srcAsset),
      },
      {
        nonce: nextNonce,
      },
    ),
  );
}
