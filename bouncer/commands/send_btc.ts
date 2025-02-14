#!/usr/bin/env -S pnpm tsx
import { sendBtc } from '../shared/send_btc';
import { globalLogger } from '../shared/utils/logger';

const bitcoinAddress = process.argv[2];
const btcAmount = parseFloat(process.argv[3]);

async function sendBitcoin() {
  try {
    await sendBtc(globalLogger, bitcoinAddress, btcAmount);
    process.exit(0);
  } catch (error) {
    console.log(`ERROR: ${error}`);
    process.exit(-1);
  }
}

await sendBitcoin();
