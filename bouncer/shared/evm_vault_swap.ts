import {
  InternalAsset as Asset,
  executeSwap,
  ExecuteSwapParams,
  approveVault,
  Asset as SCAsset,
  Chains,
  Chain,
} from '@chainflip/cli';
import { HDNodeWallet } from 'ethers';
import { randomBytes } from 'crypto';
import Keyring from '../polkadot/keyring';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  stateChainAssetFromAsset,
  createEvmWalletAndFund,
  newAddress,
} from './utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from './new_swap';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

export async function executeEvmVaultSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  wallet?: HDNodeWallet,
  brokerFees?: {
    account: string;
    commissionBps: number;
  },
): ReturnType<typeof executeSwap> {
  const srcChain = chainFromAsset(sourceAsset);
  const destChain = chainFromAsset(destAsset);
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);

  const refundAddress = await newAddress(sourceAsset, randomBytes(32).toString('hex'));
  const fokParams = fillOrKillParams ?? {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };

  const evmWallet = wallet ?? (await createEvmWalletAndFund(sourceAsset));

  const brokerComission = brokerFees ?? {
    account: new Keyring({ type: 'sr25519' }).createFromUri('//BROKER_1').address,
    commissionBps: 1,
  };

  if (erc20Assets.includes(sourceAsset)) {
    // Doing effectively infinite approvals to make sure it doesn't fail.
    // eslint-disable-next-line @typescript-eslint/no-use-before-define
    await approveEvmTokenVault(
      sourceAsset,
      (BigInt(amountToFineAmount(amountToSwap, assetDecimals(sourceAsset))) * 100n).toString(),
      evmWallet,
    );
  }

  const networkOptions = {
    signer: evmWallet,
    network: 'localnet',
    vaultContractAddress: getContractAddress(srcChain, 'VAULT'),
    srcTokenContractAddress: getContractAddress(srcChain, sourceAsset),
  } as const;
  const txOptions = {
    // This is run with fresh addresses to prevent nonce issues. Will be 1 for ERC20s.
    gasLimit: srcChain === Chains.Arbitrum ? 50000000n : 1000000n,
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
      brokerFees: brokerComission,
      // The SDK will encode these parameters and the ccmAdditionalData
      // into the `cfParameters` field for the vault swap.
      boostFeeBps,
      fillOrKillParams: fokParams,
      dcaParams,
      affiliateFees: undefined,
    } as ExecuteSwapParams,
    networkOptions,
    txOptions,
  );

  return receipt;
}

export async function approveEvmTokenVault(
  sourceAsset: Asset,
  amount: string,
  wallet: HDNodeWallet,
) {
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
