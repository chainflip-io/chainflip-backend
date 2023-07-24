#!/usr/bin/env pnpm tsx
// Constructs a very simple Raw BTC transaction. Can be used for manual testing a raw broadcast for example.
// Usage: ./commands/create_raw_btc_tx.ts <bitcoin_address> <btc_amount>

import Client from 'bitcoin-core';

const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';
console.log(`BTC_ENDPOINT is set to '${BTC_ENDPOINT}'`);

const client = new Client({
  host: BTC_ENDPOINT.split(':')[1].slice(2),
  port: Number(BTC_ENDPOINT.split(':')[2]),
  username: 'flip',
  password: 'flip',
  wallet: 'whale',
});

const createRawTransaction = async (toAddress: string, amountInBtc: number | string) => {
  try {
    const feeInBtc = 0.00001;

    // List unspent UTXOs
    const utxos = await client.listUnspent();

    const utxo = utxos.find((u) => u.amount >= Number(amountInBtc) + feeInBtc);
    if (!utxo) throw new Error('Insufficient funds');

    // Prepare the transaction inputs and outputs
    const inputs = [
      {
        txid: utxo.txid,
        vout: utxo.vout,
      },
    ];

    const changeAmount = utxo.amount - Number(amountInBtc) - feeInBtc;
    const changeAddress = await client.getNewAddress();
    const outputs = {
      [toAddress]: amountInBtc,
      [changeAddress]: changeAmount,
    };

    // Create the raw transaction
    const rawTx = await client.createRawTransaction(inputs, outputs);

    // Sign the raw transaction
    const signedTx = await client.signRawTransactionWithWallet(rawTx);

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

createRawTransaction(bitcoinAddress, btcAmount);
