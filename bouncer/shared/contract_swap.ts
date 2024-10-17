import {
  InternalAsset as Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  Asset as SCAsset,
  Chains,
  InternalAsset,
  Chain,
} from '@chainflip/cli';
import { u8aToHex, hexToU8a } from '@polkadot/util';

import { HDNodeWallet, Wallet, getDefaultProvider } from 'ethers';
import {
  observeBalanceIncrease,
  getContractAddress,
  observeCcmReceived,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  getEvmEndpoint,
  assetDecimals,
  stateChainAssetFromAsset,
  chainGasAsset,
  shortChainFromAsset,
} from './utils';
import { getBalance } from './get_balance';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from '../shared/new_swap';
import { send } from './send';
import { SwapContext, SwapStatus } from './swap_context';
import { vaultSwapCfParametersCodec } from './swapping';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

export async function executeContractSwap(
  srcAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  wallet: HDNodeWallet,
  messageMetadata?: CcmDepositMetadata,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
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

  // messageMetadata.cfParameters = undefined;
  // boostFeeBps = 1;

  const ccmAdditionalData =
    messageMetadata?.cfParameters || fillOrKillParams || dcaParams || boostFeeBps
      ? u8aToHex(
          vaultSwapCfParametersCodec.enc({
            ccmAdditionalData: messageMetadata?.cfParameters
              ? hexToU8a(messageMetadata.cfParameters)
              : undefined,
            vaultSwapAttributes:
              fillOrKillParams || dcaParams || boostFeeBps
                ? {
                    refundParams: fillOrKillParams && {
                      retryDuration: fillOrKillParams.retryDurationBlocks,
                      // refundAddress: { tag: shortChainFromAsset(srcAsset), value: hexToU8a(refundAddress) },
                      refundAddress: {
                        tag: shortChainFromAsset(srcAsset),
                        // value: fillOrKillParams.refundAddress,
                        value: hexToU8a(fillOrKillParams.refundAddress),
                      },
                      minPriceX128: BigInt(fillOrKillParams.minPriceX128),
                    },
                    dcaParams,
                    boostFee: boostFeeBps,
                  }
                : undefined,
          }),
        )
      : undefined;

  console.log('ccmAdditionalData passed to the SDK in contractCall', ccmAdditionalData);

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
      // TODO: This will need some refactoring either putting the cfParameters outside the
      // ccmParams and the user should encode it as done here or, probably better, we just
      // add the SwapAttributes support (Fok/Dca/Boost) as separate parameters, rename
      // cfParameters to ccmAdditionalData and do the encoding within the SDK.
      ccmParams: messageMetadata && {
        gasBudget: messageMetadata.gasBudget.toString(),
        message: messageMetadata.message,
        cfParameters: ccmAdditionalData,
        // ccmAdditionalData: ccmAdditionalData,
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
  swapTag = '',
  messageMetadata?: CcmDepositMetadata,
  swapContext?: SwapContext,
  log = true,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): Promise<ContractSwapParams> {
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
    await send(chainGasAsset(srcChain) as InternalAsset, wallet.address);
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
export async function approveTokenVault(srcAsset: Asset, amount: string, wallet: HDNodeWallet) {
  if (!erc20Assets.includes(srcAsset)) {
    throw new Error(`Unsupported asset, not an ERC20: ${srcAsset}`);
  }

  const chain = chainFromAsset(srcAsset as Asset);

  await approveVault(
    {
      amount,
      srcChain: chain as Chain,
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
