#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the TRON address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted as TrxUsdt.
//
// For example: ./commands/send_trxusdt.ts TAeUa4FzpoveH7UUArvfXdeS3TmXiWU1ds 1.2
// will send 1.2 TrxUsdt to account TAeUa4FzpoveH7UUArvfXdeS3TmXiWU1ds
// It also accepts non-encoded hex address representations:
// For example: ./commands/send_trxusdt.ts 0x38a4BCC04f5136e6408589A440F495D7AD0F34DB 1.2

import { runWithTimeoutAndExit, getContractAddress } from 'shared/utils';
import { sendTrc20 } from 'shared/send_trc20';
import { globalLogger } from 'shared/utils/logger';

async function main(): Promise<void> {
  const tronAddress = process.argv[2];
  const trxUsdtAmount = process.argv[3].trim();

  const contractAddress = getContractAddress('Tron', 'TrxUsdt');
  await sendTrc20(globalLogger, tronAddress, contractAddress, trxUsdtAmount);
}
await runWithTimeoutAndExit(main(), 20);
