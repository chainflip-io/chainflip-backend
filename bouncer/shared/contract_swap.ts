import {
  InternalAsset as Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  Asset as SCAsset,
  Chains,
  Chain,
} from '@chainflip/cli';
import { u32, Struct, Option, u16, u256, Bytes as TsBytes, Enum } from 'scale-ts';
import { u8aToHex, hexToU8a } from '@polkadot/util';
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
  shortChainFromAsset,
  createEvmWalletAndFund,
} from './utils';
import { getBalance } from './get_balance';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from '../shared/new_swap';
import { SwapContext, SwapStatus } from './swap_context';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

export const vaultSwapCfParametersCodec = Struct({
  ccmAdditionalData: Option(TsBytes()),
  vaultSwapParameters: Option(
    Struct({
      refundParams: Option(
        Struct({
          retryDurationBlocks: u32,
          refundAddress: Enum({
            Eth: TsBytes(20),
            Dot: TsBytes(32),
            Btc: TsBytes(),
            Arb: TsBytes(20),
            Sol: TsBytes(32),
          }),
          minPriceX128: u256,
        }),
      ),
      dcaParams: Option(Struct({ numberOfChunks: u32, chunkIntervalBlocks: u32 })),
      boostFee: Option(u16),
    }),
  ),
});

export function encodeCfParameters(
  sourceAsset: Asset,
  ccmAdditionalData?: string | undefined,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): string | undefined {
  return ccmAdditionalData || fillOrKillParams || dcaParams || boostFeeBps
    ? u8aToHex(
        vaultSwapCfParametersCodec.enc({
          ccmAdditionalData: ccmAdditionalData ? hexToU8a(ccmAdditionalData) : undefined,
          vaultSwapParameters:
            fillOrKillParams || dcaParams || boostFeeBps
              ? {
                  refundParams: fillOrKillParams && {
                    retryDurationBlocks: fillOrKillParams.retryDurationBlocks,
                    refundAddress: {
                      tag: shortChainFromAsset(sourceAsset),
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
}

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

  const cfParameters = encodeCfParameters(
    sourceAsset,
    messageMetadata?.cfParameters,
    boostFeeBps,
    fillOrKillParams,
    dcaParams,
  );

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
      // TODO: This will need some refactoring either putting the cfParameters outside the
      // ccmParams and the user should encode it as done here or, probably better, we just
      // add the SwapParameters support (Fok/Dca/Boost) as separate parameters, rename
      // cfParameters to ccmAdditionalData and do the encoding within the SDK.
      ccmParams: messageMetadata && {
        gasBudget: messageMetadata.gasBudget.toString(),
        message: messageMetadata.message,
        cfParameters,
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
