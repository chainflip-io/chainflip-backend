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
import BigNumber from 'bignumber.js';
import Web3 from 'web3';
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
  createStateChainKeypair,
  decodeDotAddressForContract,
  getEvmEndpoint,
} from './utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from './new_swap';
import { getChainflipApi } from './utils/substrate';
import { ChannelRefundParameters } from './sol_vault_swap';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'ArbUsdc'];

interface EvmVaultSwapDetails {
  chain: 'Ethereum' | 'Arbitrum';
  calldata: string;
  value: string;
  to: string;
}

interface EvmVaultSwapExtraParameters {
  chain: 'Ethereum' | 'Arbitrum';
  input_amount: string;
  refund_parameters: ChannelRefundParameters;
}

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
): Promise<string> {
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

  const fineAmount = amountToFineAmount(amountToSwap, assetDecimals(sourceAsset));

  if (Math.random() > 0.5) {
    // Use SDK
    const networkOptions = {
      signer: evmWallet,
      network: 'localnet',
      vaultContractAddress: getContractAddress(srcChain, 'VAULT'),
      srcTokenContractAddress: getContractAddress(srcChain, sourceAsset),
    } as const;
    const txOptions = {
      // This is run with fresh addresses to prevent nonce issues. Will be 1 for ERC20s.
      gasLimit: srcChain === Chains.Arbitrum ? 32000000n : 5000000n,
    } as const;

    const receipt = await executeSwap(
      {
        destChain,
        destAsset: stateChainAssetFromAsset(destAsset),
        // It is important that this is large enough to result in
        // an amount larger than existential (e.g. on Polkadot):
        amount: fineAmount,
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
    return receipt.hash;
  }

  // Use the broker API
  await using chainflip = await getChainflipApi();
  const brokerUri = '//BROKER_1';
  const broker = createStateChainKeypair(brokerUri);

  const refundParams: ChannelRefundParameters = {
    retry_duration: fokParams.retryDurationBlocks,
    refund_address: fokParams.refundAddress,
    min_price: '0x' + new BigNumber(fokParams.minPriceX128).toString(16),
  };

  const extraParameters: EvmVaultSwapExtraParameters = {
    chain: srcChain as 'Ethereum' | 'Arbitrum',
    input_amount: '0x' + new BigNumber(fineAmount).toString(16),
    refund_parameters: refundParams,
  };

  const vaultSwapDetails = (await chainflip.rpc(
    `cf_get_vault_swap_details`,
    broker.address,
    { chain: chainFromAsset(sourceAsset), asset: stateChainAssetFromAsset(sourceAsset) },
    { chain: chainFromAsset(destAsset), asset: stateChainAssetFromAsset(destAsset) },
    chainFromAsset(destAsset) === Chains.Polkadot
      ? decodeDotAddressForContract(destAddress)
      : destAddress,
    0, // broker_commission
    extraParameters, // extra_parameters
    // channel_metadata
    messageMetadata && {
      message: messageMetadata.message as `0x${string}`,
      gas_budget: messageMetadata.gasBudget,
      ccm_additional_data: messageMetadata.ccmAdditionalData,
    },
    boostFeeBps ?? 0, // boost_fee
    null, // affiliates
    dcaParams && {
      number_of_chunks: dcaParams.numberOfChunks,
      chunk_interval: dcaParams.chunkIntervalBlocks,
    },
  )) as unknown as EvmVaultSwapDetails;

  const web3 = new Web3(getEvmEndpoint(srcChain));
  const tx = {
    to: vaultSwapDetails.to,
    data: vaultSwapDetails.calldata,
    value: new BigNumber(vaultSwapDetails.value.slice(2), 16).toString(),
    gas: srcChain === 'Arbitrum' ? 32000000 : 5000000,
  };

  const signedTx = await web3.eth.accounts.signTransaction(tx, evmWallet.privateKey);
  const receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);

  return receipt.transactionHash;
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
