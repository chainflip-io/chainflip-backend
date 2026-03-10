#!/usr/bin/env -S pnpm tsx
// Constructs a very simple Raw BTC transaction. Can be used for manual testing a raw broadcast for example.
// Usage: ./commands/create_raw_btc_tx.ts <bitcoin_address> <btc_amount>

import { BTC_ENDPOINT, globalBtcWhaleMutexClient } from 'shared/send_btc';

console.log(`Btc endpoint is set to '${BTC_ENDPOINT}'`);

const createRawTransaction = async (toAddress: string, amountInBtc: number | string) => {
  try {
    // Create the raw transaction
    const rawTx = await globalBtcWhaleMutexClient.unsafe_getClient().createRawTransaction([], {
      [toAddress]: amountInBtc,
    });
    const fundedTx = (await globalBtcWhaleMutexClient.unsafe_getClient().fundRawTransaction(rawTx, {
      changeAddress: await globalBtcWhaleMutexClient.getNewAddress(),
      feeRate: 0.00001,
      lockUnspents: true,
    })) as { hex: string };

    // Sign the raw transaction
    const signedTx = await globalBtcWhaleMutexClient
      .unsafe_getClient()
      .signRawTransactionWithWallet(fundedTx);

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
