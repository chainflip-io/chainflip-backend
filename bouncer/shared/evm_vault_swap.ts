import { InternalAsset as Asset, Chains } from '@chainflip/cli';
import { Contract, HDNodeWallet } from 'ethers';
import { randomBytes } from 'crypto';
import BigNumber from 'bignumber.js';
import Web3 from 'web3';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  stateChainAssetFromAsset,
  createEvmWalletAndFund,
  newAssetAddress,
  decodeDotAddressForContract,
  getEvmEndpoint,
} from 'shared/utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';
import { getChainflipApi } from 'shared/utils/substrate';
import { ChannelRefundParameters } from 'shared/sol_vault_swap';
import { Logger } from 'shared/utils/logger';
import { getErc20abi } from 'shared/contract_interfaces';

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
  logger: Logger,
  brokerAddress: string,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  brokerCommissionBps: number = 0,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  wallet?: HDNodeWallet,
  affiliateFees: {
    accountAddress: string;
    commissionBps: number;
  }[] = [],
  optionalRefundAddress?: string,
) {
  const srcChain = chainFromAsset(sourceAsset);
  const destChain = chainFromAsset(destAsset);
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);
  const refundAddress =
    optionalRefundAddress ?? (await newAssetAddress(sourceAsset, randomBytes(32).toString('hex')));
  const fokParams = fillOrKillParams ?? {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };
  const fineAmount = amountToFineAmount(amountToSwap, assetDecimals(sourceAsset));
  const evmWallet = wallet ?? (await createEvmWalletAndFund(logger, sourceAsset));

  if (erc20Assets.includes(sourceAsset)) {
    // Doing effectively infinite approvals to make sure it doesn't fail.
    // eslint-disable-next-line @typescript-eslint/no-use-before-define
    await approveEvmTokenVault(
      sourceAsset,
      (BigInt(amountToFineAmount(amountToSwap, assetDecimals(sourceAsset))) * 100n).toString(),
      evmWallet,
    );
  }

  await using chainflip = await getChainflipApi();

  const refundParams: ChannelRefundParameters = {
    retry_duration: fokParams.retryDurationBlocks,
    refund_address: fokParams.refundAddress,
    min_price: '0x' + new BigNumber(fokParams.minPriceX128).toString(16),
    refund_ccm_metadata: fillOrKillParams?.refundCcmMetadata && {
      message: fillOrKillParams?.refundCcmMetadata.message as `0x${string}`,
      gas_budget: fillOrKillParams?.refundCcmMetadata.gasBudget,
      ccm_additional_data: fillOrKillParams?.refundCcmMetadata.ccmAdditionalData,
    },
  };

  const extraParameters: EvmVaultSwapExtraParameters = {
    chain: srcChain as 'Ethereum' | 'Arbitrum',
    input_amount: '0x' + new BigNumber(fineAmount).toString(16),
    refund_parameters: refundParams,
  };

  logger.trace('Requesting vault swap parameter encoding');
  const vaultSwapDetails = (await chainflip.rpc(
    `cf_request_swap_parameter_encoding`,
    brokerAddress,
    { chain: srcChain, asset: stateChainAssetFromAsset(sourceAsset) },
    { chain: destChain, asset: stateChainAssetFromAsset(destAsset) },
    destChain === Chains.Polkadot || destChain === Chains.Assethub
      ? decodeDotAddressForContract(destAddress)
      : destAddress,
    brokerCommissionBps,
    extraParameters,
    messageMetadata && {
      message: messageMetadata.message as `0x${string}`,
      gas_budget: messageMetadata.gasBudget,
      ccm_additional_data: messageMetadata.ccmAdditionalData,
    },
    boostFeeBps ?? 0,
    affiliateFees.map((fee) => ({
      account: fee.accountAddress,
      bps: fee.commissionBps,
    })),
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

  logger.trace('Signing and Sending EVM vault swap transaction');
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

  const erc20abi = await getErc20abi();
  const chain = chainFromAsset(sourceAsset);
  const tokenContractAddress = getContractAddress(chain, sourceAsset);
  const sourceTokenContract = new Contract(tokenContractAddress, erc20abi, wallet);

  const approvalTx = await sourceTokenContract.approve(
    getContractAddress(chain, 'VAULT'),
    amount,
    // This is run with fresh addresses to prevent nonce issues
    { nonce: 0 },
  );
  await approvalTx.wait();
}
