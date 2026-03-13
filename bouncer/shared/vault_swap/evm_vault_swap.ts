import { Contract, HDNodeWallet } from 'ethers';
import { randomBytes } from 'crypto';
import BigNumber from 'bignumber.js';
import {
  getContractAddress,
  amountToFineAmount,
  defaultAssetAmounts,
  chainFromAsset,
  assetDecimals,
  createEvmWalletAndFund,
  newAssetAddress,
  Asset,
} from 'shared/utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';
import { getChainflipApi } from 'shared/utils/substrate';
import { getErc20abi } from 'shared/contract_interfaces';
import { ChainflipIO, WithBrokerAccount } from 'shared/utils/chainflip_io';
import { signAndSendTxEvm } from 'shared/send_evm';
import { ChannelRefundParameters } from './sol_vault_swap';
import { requestSwapParameterEncoding } from './vault_swap';

const erc20Assets: Asset[] = ['Flip', 'Usdc', 'Usdt', 'Wbtc', 'ArbUsdc', 'ArbUsdt'];

interface EvmVaultSwapDetails {
  chain: 'Ethereum' | 'Arbitrum';
  calldata: string;
  value: string;
  to: string;
}

interface VaultSwapExtraParameters {
  chain: string;
  input_amount: string;
  refund_parameters: ChannelRefundParameters;
}

export async function requestEvmSwapParameterEncoding<A extends WithBrokerAccount, T>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  brokerCommissionBps: number,
  messageMetadata: CcmDepositMetadata | undefined,
  boostFeeBps: number,
  affiliateFees: { accountAddress: string; commissionBps: number }[],
  dcaParams: DcaParams | undefined,
  fillOrKillParams: FillOrKillParamsX128 | undefined,
  amount: string | undefined,
  optionalRefundAddress: string | undefined,
): Promise<T> {
  const srcChain = chainFromAsset(sourceAsset);
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);
  const refundAddress =
    optionalRefundAddress ?? (await newAssetAddress(sourceAsset, randomBytes(32).toString('hex')));
  const fokParams = fillOrKillParams ?? {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };
  const fineAmount = amountToFineAmount(amountToSwap, assetDecimals(sourceAsset));

  await using chainflip = await getChainflipApi();

  const refundParams: ChannelRefundParameters = {
    retry_duration: fokParams.retryDurationBlocks,
    refund_address: fokParams.refundAddress,
    min_price: '0x' + new BigNumber(fokParams.minPriceX128).toString(16),
    refund_ccm_metadata: fillOrKillParams?.refundCcmMetadata
      ? {
          message: fillOrKillParams.refundCcmMetadata.message,
          gas_budget: fillOrKillParams.refundCcmMetadata.gasBudget,
          ccm_additional_data: fillOrKillParams.refundCcmMetadata.ccmAdditionalData,
        }
      : undefined,
    max_oracle_price_slippage: undefined,
  };

  const extraParameters: VaultSwapExtraParameters = {
    chain: srcChain,
    input_amount: '0x' + new BigNumber(fineAmount).toString(16),
    refund_parameters: refundParams,
  };

  cf.debug('Requesting vault swap parameter encoding');
  const vaultSwapDetails = await requestSwapParameterEncoding<EvmVaultSwapDetails>(
    chainflip,
    cf.requirements.account.keypair.address,
    sourceAsset,
    destAsset,
    destAddress,
    brokerCommissionBps,
    extraParameters,
    messageMetadata,
    boostFeeBps ?? 0,
    affiliateFees.map((fee) => ({ account: fee.accountAddress, bps: fee.commissionBps })),
    dcaParams,
  );

  const tx = {
    to: vaultSwapDetails.to,
    data: vaultSwapDetails.calldata,
    value: new BigNumber(vaultSwapDetails.value.slice(2), 16).toString(),
    gas: srcChain === 'Arbitrum' ? 32000000 : 5000000,
  };

  cf.debug('Signing and Sending EVM vault swap transaction');
  const receipt = await signAndSendTxEvm(cf.logger, srcChain, tx, {
    privateKey: evmWallet.privateKey,
  });

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
