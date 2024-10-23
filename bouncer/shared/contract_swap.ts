import {
  InternalAsset as Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  Asset as SCAsset,
  Chains,
  Chain,
} from '@chainflip/cli';
import { HDNodeWallet, Wallet } from 'ethers';
import {
  observeBalanceIncrease,
  getContractAddress,
  observeCcmReceived,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  stateChainAssetFromAsset,
  createEvmWalletAndFund,
} from './utils';
import { getBalance } from './get_balance';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from '../shared/new_swap';
import { SwapContext, SwapStatus } from './swap_context';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

export async function executeContractSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  wallet: HDNodeWallet,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): ReturnType<typeof executeSwap> {
  const srcChain = chainFromAsset(sourceAsset);
  const destChain = chainFromAsset(destAsset);
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    vaultContractAddress: getContractAddress(srcChain, 'VAULT'),
    srcTokenContractAddress: getContractAddress(srcChain, sourceAsset),
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
      amount: amountToFineAmount(amountToSwap, assetDecimals(sourceAsset)),
      destAddress,
      srcAsset: stateChainAssetFromAsset(sourceAsset),
      srcChain,
      ccmParams: messageMetadata && {
        gasBudget: messageMetadata.gasBudget.toString(),
        message: messageMetadata.message,
        ccmAdditionalData: messageMetadata.ccmAdditionalData,
      },
      // The SDK will encode these parameters and the ccmAdditionalData
      // into the `cfParameters` field for the vault swap.
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
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
  swapTag = '',
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  log = true,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): Promise<ContractSwapParams> {
  const tag = swapTag ?? '';
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);

  // Generate a new wallet for each contract swap to prevent nonce issues when running in parallel
  // with other swaps via deposit channels.
  const mnemonic = Wallet.createRandom().mnemonic?.phrase ?? '';
  if (mnemonic === '') {
    throw new Error('Failed to create random mnemonic');
  }
  const wallet = await createEvmWalletAndFund(sourceAsset);

  try {
    if (erc20Assets.includes(sourceAsset)) {
      // Doing effectively infinite approvals to make sure it doesn't fail.
      // eslint-disable-next-line @typescript-eslint/no-use-before-define
      await approveTokenVault(
        sourceAsset,
        (BigInt(amountToFineAmount(amountToSwap, assetDecimals(sourceAsset))) * 100n).toString(),
        wallet,
      );
    }
    swapContext?.updateStatus(swapTag, SwapStatus.ContractApproved);

    const oldBalance = await getBalance(destAsset, destAddress);
    if (log) {
      console.log(`${tag} Old balance: ${oldBalance}`);
      console.log(
        `${tag} Executing (${sourceAsset}) contract swap to(${destAsset}) ${destAddress}. Current balance: ${oldBalance}`,
      );
    }

    // To uniquely identify the contractSwap, we need to use the TX hash. This is only known
    // after sending the transaction, so we send it first and observe the events afterwards.
    // There are still multiple blocks of safety margin inbetween before the event is emitted
    const receipt = await executeContractSwap(
      sourceAsset,
      destAsset,
      destAddress,
      wallet,
      messageMetadata,
      amountToSwap,
      boostFeeBps,
      fillOrKillParams,
      dcaParams,
    );
    swapContext?.updateStatus(swapTag, SwapStatus.ContractExecuted);

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
    if (log) {
      console.log(`${tag} Swap success! New balance: ${newBalance}!`);
    }
    swapContext?.updateStatus(swapTag, SwapStatus.Success);
    return {
      sourceAsset,
      destAsset,
      destAddress,
      txHash: receipt.hash,
    };
  } catch (err) {
    console.error('err:', err);
    swapContext?.updateStatus(swapTag, SwapStatus.Failure);
    if (err instanceof Error) {
      console.log(err.stack);
    }
    throw new Error(`${tag} ${err}`);
  }
}
export async function approveTokenVault(sourceAsset: Asset, amount: string, wallet: HDNodeWallet) {
  if (!erc20Assets.includes(sourceAsset)) {
    throw new Error(`Unsupported asset, not an ERC20: ${sourceAsset}`);
  }

  const chain = chainFromAsset(sourceAsset as Asset);

  await approveVault(
    {
      amount,
      srcChain: chain as Chain,
      srcAsset: stateChainAssetFromAsset(sourceAsset) as SCAsset,
    },
    {
      signer: wallet,
      network: 'localnet',
      vaultContractAddress: getContractAddress(chain, 'VAULT'),
      srcTokenContractAddress: getContractAddress(chain, sourceAsset),
    },
    // This is run with fresh addresses to prevent nonce issues
    {
      nonce: 0,
    },
  );
}
