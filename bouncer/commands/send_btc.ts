#!/usr/bin/env -S pnpm tsx
import { sendBtc } from '../shared/send_btc';

const bitcoinAddress = process.argv[2];
const btcAmount = parseFloat(process.argv[3]);

async function sendBitcoin() {
  try {
    await sendBtc(bitcoinAddress, btcAmount);
    process.exit(0);
  } catch (error) {
    console.log(`ERROR: ${error}`);
    process.exit(-1);
  }
}

sendBitcoin();
