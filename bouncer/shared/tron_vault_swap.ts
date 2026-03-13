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
} from 'shared/utils';
import { CcmDepositMetadata, DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';
import { requestEvmSwapParameterEncoding } from 'shared/evm_vault_swap';
import { ChainflipIO, WithBrokerAccount } from './utils/chainflip_io';

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
  const amountToSwap = amount ?? defaultAssetAmounts(sourceAsset);
  const vaultSwapDetails = await requestEvmSwapParameterEncoding<A, TronVaultSwapDetails>(
    cf,
    sourceAsset,
    destAsset,
    destAddress,
    brokerCommissionBps,
    messageMetadata,
    boostFeeBps ?? 0,
    affiliateFees,
    dcaParams,
    fillOrKillParams,
    amountToSwap,
    optionalRefundAddress,
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
