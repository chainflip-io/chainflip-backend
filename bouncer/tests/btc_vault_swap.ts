import { ExecutableTest } from '../shared/executable_test';
import { BTC_ENDPOINT, selectInputs, waitForBtcTransaction, btcClient } from '../shared/send_btc';
import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  btcClientMutex,
  createStateChainKeypair,
  newAddress,
  shortChainFromAsset,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testBtcVaultSwap = new ExecutableTest('Btc-Vault-Swap', main, 60);

interface EncodedSwapRequest {
  Bitcoin: {
    nulldata_utxo: string;
  };
}

interface VaultSwapDetails {
  deposit_address: string;
  encoded_params: EncodedSwapRequest;
}

async function buildAndSendBtcVaultSwap(
  depositAmountBtc: number,
  brokerUri: string,
  destinationAsset: Asset,
  destinationAddress: string,
  refundAddress: string,
) {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair(brokerUri);
  testBtcVaultSwap.debugLog(`Btc endpoint is set to`, BTC_ENDPOINT);

  const feeBtc = 0.00001;
  const { inputs, change } = await selectInputs(Number(depositAmountBtc) + feeBtc);

  const vaultSwapDetails = (await chainflip.rpc(
    `cf_get_vault_swap_details`,
    broker.address,
    'BTC', // source_asset
    destinationAsset.toUpperCase(),
    { [shortChainFromAsset(destinationAsset)]: destinationAddress },
    0, // broker_commission
    0, // min_output_amount
    0, // retry_duration
  )) as unknown as VaultSwapDetails;
  testBtcVaultSwap.debugLog(
    'nulldata_utxo:',
    vaultSwapDetails.encoded_params.Bitcoin.nulldata_utxo,
  );

  // The `createRawTransaction` function will add the op codes, so we have to remove them here.
  const nullDataWithoutOpCodes = vaultSwapDetails.encoded_params.Bitcoin.nulldata_utxo
    .replace('0x', '')
    .substring(4);

  const outputs = [
    {
      [vaultSwapDetails.deposit_address]: depositAmountBtc,
    },
    {
      data: nullDataWithoutOpCodes,
    },
    {
      [refundAddress]: change,
    },
  ];

  const rawTx = await btcClient.createRawTransaction(inputs, outputs, 0, false);
  const signedTx = await btcClient.signRawTransactionWithWallet(rawTx);
  const txid = await btcClientMutex.runExclusive(async () =>
    btcClient.sendRawTransaction(signedTx.hex),
  );
  if (!txid) {
    throw new Error('Broadcast failed');
  } else {
    testBtcVaultSwap.log('Broadcast successful, txid:', txid);
  }

  await waitForBtcTransaction(txid as string);
  testBtcVaultSwap.debugLog('Transaction confirmed');
}

async function testVaultSwap(depositAmountBtc: number, brokerUri: string, destinationAsset: Asset) {
  const destinationAddress = await newAddress(destinationAsset, 'BTC_VAULT_SWAP');
  testBtcVaultSwap.debugLog('destinationAddress:', destinationAddress);
  const refundAddress = await newAddress('Btc', 'BTC_VAULT_SWAP_REFUND');
  testBtcVaultSwap.debugLog('Refund address:', refundAddress);

  const observeSwapExecutedEvent = observeEvent(`swapping:SwapExecuted`, {
    test: (event) =>
      event.data.inputAsset === 'Btc' &&
      event.data.outputAsset === destinationAsset &&
      event.data.inputAmount.replace(/,/g, '') ===
        amountToFineAmount(depositAmountBtc.toString(), assetDecimals('Btc')),
  }).event;

  await buildAndSendBtcVaultSwap(
    depositAmountBtc,
    brokerUri,
    destinationAsset,
    destinationAddress,
    refundAddress,
  );

  testBtcVaultSwap.debugLog('Waiting for swap executed event');
  await observeSwapExecutedEvent;
  testBtcVaultSwap.log(`✅ Btc -> ${destinationAsset} Vault Swap executed`);
}

async function main() {
  const depositAmount = 0.1;

  await testVaultSwap(depositAmount, '//BROKER_1', 'Flip');
}
