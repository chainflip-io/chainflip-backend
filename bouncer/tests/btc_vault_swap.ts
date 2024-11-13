import assert from 'assert';
import { ExecutableTest } from '../shared/executable_test';
import { BTC_ENDPOINT, selectInputs, waitForBtcTransaction, btcClient } from '../shared/send_btc';
import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  btcClientMutex,
  createStateChainKeypair,
  newAddress,
  observeBalanceIncrease,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';
import { jsonRpc } from '../shared/json_rpc';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testBtcVaultSwap = new ExecutableTest('Btc-Vault-Swap', main, 100);

interface VaultSwapDetails {
  chain: string;
  nulldata_utxo: string;
  deposit_address: string;
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
  testBtcVaultSwap.debugLog('Broker:', broker.address);
  testBtcVaultSwap.debugLog(`Btc endpoint is set to`, BTC_ENDPOINT);

  const feeBtc = 0.00001;
  const { inputs, change } = await selectInputs(Number(depositAmountBtc) + feeBtc);

  const vaultSwapDetails = (await chainflip.rpc(
    `cf_get_vault_swap_details`,
    broker.address,
    'BTC', // source_asset
    destinationAsset.toUpperCase(),
    destinationAddress,
    0, // broker_commission
    0, // min_output_amount
    0, // retry_duration
  )) as unknown as VaultSwapDetails;

  assert.strictEqual(vaultSwapDetails.chain, 'Bitcoin');
  testBtcVaultSwap.debugLog('nulldata_utxo:', vaultSwapDetails.nulldata_utxo);
  testBtcVaultSwap.debugLog('deposit_address:', vaultSwapDetails.deposit_address);

  // The `createRawTransaction` function will add the op codes, so we have to remove them here.
  const nullDataWithoutOpCodes = vaultSwapDetails.nulldata_utxo.replace('0x', '').substring(4);

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
  }
  testBtcVaultSwap.log('Broadcast successful, txid:', txid);

  await waitForBtcTransaction(txid as string);
  testBtcVaultSwap.debugLog('Transaction confirmed');
}

async function testVaultSwap(depositAmountBtc: number, brokerUri: string, destinationAsset: Asset) {
  const destinationAddress = await newAddress(destinationAsset, 'BTC_VAULT_SWAP');
  testBtcVaultSwap.debugLog('destinationAddress:', destinationAddress);
  const refundAddress = await newAddress('Btc', 'BTC_VAULT_SWAP_REFUND');
  testBtcVaultSwap.debugLog('Refund address:', refundAddress);
  const destinationAmountBeforeSwap = await getBalance(destinationAsset, destinationAddress);

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
  testBtcVaultSwap.log(`Btc -> ${destinationAsset} Vault Swap executed`);

  await observeBalanceIncrease(destinationAsset, destinationAddress, destinationAmountBeforeSwap);
  testBtcVaultSwap.log(`Balance increased, Vault Swap Complete`);
}

async function openPrivateBtcChannel(brokerUri: string) {
  // TODO: Use chainflip SDK instead so we can support any broker uri
  assert.strictEqual(brokerUri, '//BROKER_1', 'Support for other brokers is not implemented');

  // TODO: use chainflip SDK to check if the channel is already open
  try {
    await jsonRpc('broker_open_private_btc_channel', [], 'http://127.0.0.1:10997');
    testBtcVaultSwap.log('Private Btc channel opened');
  } catch (error) {
    // We expect this to fail if the channel already exists from a previous run
    testBtcVaultSwap.debugLog('Failed to open private Btc channel', error);
  }
}

async function main() {
  const btcDepositAmount = 0.1;
  const brokerUri = '//BROKER_1';

  await openPrivateBtcChannel(brokerUri);

  await testVaultSwap(btcDepositAmount, brokerUri, 'Flip');
}
