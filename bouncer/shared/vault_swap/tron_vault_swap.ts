import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  chainFromAsset,
  defaultAssetAmounts,
  getContractAddress,
  getEncodedTronAddress,
  getTronWebClient,
  getTronWhaleKeyPair,
  newAssetAddress,
} from 'shared/utils';
import BigNumber from 'bignumber.js';
import { randomBytes } from 'crypto';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';
import { ChainflipIO, WithBrokerAccount } from 'shared/utils/chainflip_io';
import { getChainflipApi } from 'shared/utils/substrate';
import { ChannelRefundParameters, requestSwapParameterEncoding } from './vault_swap';
import { EvmVaultSwapExtraParameters } from './evm_vault_swap';

interface TronVaultSwapDetails {
  chain: 'Tron';
  calldata: string;
  value: string;
  to: string;
  note: string;
  source_token_address?: string;
}

export async function executeTronVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  brokerCommissionBps: number = 0,
  messageMetadata?: CcmDepositMetadata,
  amount?: string,
  boostFeeBps?: number,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
  affiliateFees: {
    accountAddress: string;
    commissionBps: number;
  }[] = [],
  optionalRefundAddress?: string,
) {
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

  const extraParameters: EvmVaultSwapExtraParameters = {
    chain: srcChain as 'Ethereum' | 'Arbitrum',
    input_amount: '0x' + new BigNumber(fineAmount).toString(16),
    refund_parameters: refundParams,
  };

  cf.debug('Requesting vault swap parameter encoding');
  const vaultSwapDetails = await requestSwapParameterEncoding<TronVaultSwapDetails>(
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

  const tronWeb = getTronWebClient();
  const { privkey, pubkey } = getTronWhaleKeyPair();

  let transaction;
  if (sourceAsset === 'Trx') {
    if (vaultSwapDetails.calldata && vaultSwapDetails.calldata !== '0x') {
      throw new Error('Native TRX vault swaps should not have calldata');
    }
    // Create a native TRX transfer transaction
    transaction = await tronWeb.transactionBuilder.sendTrx(
      getEncodedTronAddress(vaultSwapDetails.to),
      Number(vaultSwapDetails.value),
      getEncodedTronAddress(pubkey),
    );
  } else {
    // TRC20 vault swap: transfer tokens to vaultSwapDetails.to
    const tokenContractAddress = getEncodedTronAddress(
      getContractAddress(chainFromAsset(sourceAsset), sourceAsset),
    );
    if (tokenContractAddress.slice(2) !== vaultSwapDetails.source_token_address?.slice(2))
      throw new Error(
        `Source token address mismatch. Expected ${tokenContractAddress}, got ${vaultSwapDetails.source_token_address}`,
      );

    const result = await tronWeb.transactionBuilder.triggerSmartContract(
      tokenContractAddress,
      'transfer(address,uint256)',
      { feeLimit: 100_000_000 },
      [
        { type: 'address', value: getEncodedTronAddress(vaultSwapDetails.to) },
        { type: 'uint256', value: amountToFineAmount(amountToSwap, assetDecimals(sourceAsset)) },
      ],
      getEncodedTronAddress(pubkey),
    );
    const calldata = result.transaction.raw_data.contract[0].parameter.value.data;

    // Check that the calldata matches if a user were to use the raw calldata to build the transaction.
    if (calldata !== vaultSwapDetails.calldata.slice(2)) {
      throw new Error(
        `Calldata mismatch. Expected ${vaultSwapDetails.calldata}, got 0x${calldata}`,
      );
    }
    transaction = result.transaction;
  }

  // Add memo/note to the transaction using addUpdateData
  transaction = await tronWeb.transactionBuilder.addUpdateData(
    transaction,
    vaultSwapDetails.note.substring(2),
    'hex',
  );

  // Sign and broadcast
  const signedTx = await tronWeb.trx.sign(transaction, privkey);
  const broadcast = await tronWeb.trx.sendRawTransaction(signedTx);
  return '0x' + broadcast.txid;
}
