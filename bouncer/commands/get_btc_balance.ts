#!/usr/bin/env -S pnpm tsx
import { getBtcBalance } from '../shared/get_btc_balance';

export async function getBtcBalanceCommand(bitcoinAddress: string) {
  try {
    const amount = await getBtcBalance(bitcoinAddress);
    console.log(amount);
  } catch (error) {
    console.log(`ERROR: ${error}`);
    process.exit(-1);
  }

  process.exit(0);
}

const bitcoinAddress = process.argv[2];

if (!bitcoinAddress) {
  console.log('Must provide an address to query');
  process.exit(-1);
}

getBtcBalanceCommand(bitcoinAddress);
