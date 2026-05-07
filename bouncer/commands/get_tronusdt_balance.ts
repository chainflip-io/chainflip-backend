#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the TrxUsdt balance of the address provided as the first argument.
//
// For example: ./commands/get_trxusdt_balance.ts TAeUa4FzpoveH7UUArvfXdeS3TmXiWU1ds
// might print: 100.2

import { runWithTimeoutAndExit, getContractAddress } from 'shared/utils';
import { getTrc20Balance } from 'shared/get_trc20_balance';

async function getTrxUsdtBalanceCommand(tronAddress: string) {
  const contractAddress = getContractAddress('Tron', 'TrxUsdt');
  console.log(await getTrc20Balance(tronAddress, contractAddress));
}

const tronAddress = process.argv[2] ?? '0';
await runWithTimeoutAndExit(getTrxUsdtBalanceCommand(tronAddress), 5);
