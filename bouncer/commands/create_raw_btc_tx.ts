#!/usr/bin/env -S pnpm tsx
// Constructs a very simple Raw BTC transaction. Can be used for manual testing a raw broadcast for example.
// Usage: ./commands/create_raw_btc_tx.ts <bitcoin_address> <btc_amount>

import { BTC_ENDPOINT, btcClient, selectInputs } from '../shared/send_btc';

console.log(`Btc endpoint is set to '${BTC_ENDPOINT}'`);

const createRawTransaction = async (toAddress: string, amountInBtc: number | string) => {
  try {
    // Inputs and outputs
    const feeInBtc = 0.00001;
    const { inputs, change } = await selectInputs(Number(amountInBtc) + feeInBtc);
    const changeAddress = await btcClient.getNewAddress();
    const outputs = {
      [toAddress]: amountInBtc,
      [changeAddress]: change,
    };

    // Create the raw transaction
    const rawTx = await btcClient.createRawTransaction(inputs, outputs);

    // Sign the raw transaction
    const signedTx = await btcClient.signRawTransactionWithWallet(rawTx);

    // Here's your raw signed transaction
    console.log('Raw signed transaction:', signedTx.hex);
  } catch (error) {
    console.error('An error occurred', error);
  }
};

const bitcoinAddress = process.argv[2];
const btcAmount = parseFloat(process.argv[3]);

if (!bitcoinAddress || !btcAmount) {
  console.log('Usage: pnpm tsx create_raw_btc_tx.js <bitcoin_address> <btc_amount>');
  process.exit(-1);
}

await createRawTransaction(bitcoinAddress, btcAmount);
