import {
  InternalAsset as Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  Asset as SCAsset,
  Chains,
} from '@chainflip/cli';
import { HDNodeWallet, Wallet, getDefaultProvider } from 'ethers';
import {
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  getContractAddress,
  observeCcmReceived,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  getEvmEndpoint,
  assetDecimals,
  stateChainAssetFromAsset,
  chainGasAsset,
} from './utils';
import { getBalance } from './get_balance';
import { CcmDepositMetadata } from '../shared/new_swap';
import { send } from './send';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

export async function executeContractSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  wallet: HDNodeWallet,
  messageMetadata?: CcmDepositMetadata,
): ReturnType<typeof executeSwap> {
  const srcChain = chainFromAsset(srcAsset);
  const destChain = chainFromAsset(destAsset);

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    vaultContractAddress: getContractAddress(srcChain, 'VAULT'),
    srcTokenContractAddress: getContractAddress(srcChain, srcAsset),
  } as const;
  const txOptions = {
    // This is run with fresh addresses to prevent nonce issues. Will be 1 for ERC20s.
    gasLimit: srcChain === Chains.Arbitrum ? 10000000n : 200000n,
  } as const;

  const receipt = await executeSwap(
    {
      destChain,
      destAsset: stateChainAssetFromAsset(destAsset),
      // It is important that this is large enough to result in
      // an amount larger than existential (e.g. on Polkadot):
      amount: amountToFineAmount(defaultAssetAmounts(srcAsset), assetDecimals(srcAsset)),
      destAddress,
      srcAsset: stateChainAssetFromAsset(srcAsset),
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

  const srcChain = chainFromAsset(sourceAsset);

  // Generate a new wallet for each contract swap to prevent nonce issues when running in parallel
  // with other swaps via deposit channels.
  const mnemonic = Wallet.createRandom().mnemonic?.phrase ?? '';
  if (mnemonic === '') {
    throw new Error('Failed to create random mnemonic');
  }
  const wallet = Wallet.fromPhrase(mnemonic).connect(getDefaultProvider(getEvmEndpoint(srcChain)));

  try {
    // Fund new key with native asset and asset to swap.
    await send(chainGasAsset(srcChain), wallet.address);
    await send(sourceAsset, wallet.address);

    if (erc20Assets.includes(sourceAsset)) {
      // Doing effectively infinite approvals to make sure it doesn't fail.
      // eslint-disable-next-line @typescript-eslint/no-use-before-define
      await approveTokenVault(
        sourceAsset,
        (
          BigInt(amountToFineAmount(defaultAssetAmounts(sourceAsset), assetDecimals(sourceAsset))) *
          100n
        ).toString(),
        wallet,
      );
    }

    const oldBalance = await getBalance(destAsset, destAddress);
    console.log(`${tag} Old balance: ${oldBalance}`);
    console.log(
      `${tag} Executing (${sourceAsset}) contract swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
    );

    // To uniquely identify the contractSwap, we need to use the TX hash. This is only known
    // after sending the transaction, so we send it first and observe the events afterwards.
    // There are still multiple blocks of safety margin inbetween before the event is emitted
    const receipt = await executeContractSwap(
      sourceAsset,
      destAsset,
      destAddress,
      wallet,
      messageMetadata,
    );
    await observeEvent('swapping:SwapScheduled', api, (event) => {
      if ('Vault' in event.data.origin) {
        const sourceAssetMatches = sourceAsset === (event.data.sourceAsset as Asset);
        const destAssetMatches = destAsset === (event.data.destinationAsset as Asset);
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
          wallet.address.toLowerCase(),
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
export async function approveTokenVault(srcAsset: Asset, amount: string, wallet: HDNodeWallet) {
  if (!erc20Assets.includes(srcAsset)) {
    throw new Error(`Unsupported asset, not an ERC20: ${srcAsset}`);
  }

  const chain = chainFromAsset(srcAsset as Asset);

  await approveVault(
    {
      amount,
      srcChain: chain,
      srcAsset: stateChainAssetFromAsset(srcAsset) as SCAsset,
    },
    {
      signer: wallet,
      network: 'localnet',
      vaultContractAddress: getContractAddress(chain, 'VAULT'),
      srcTokenContractAddress: getContractAddress(chain, srcAsset),
    },
    // This is run with fresh addresses to prevent nonce issues
    {
      nonce: 0,
    },
  );
}
