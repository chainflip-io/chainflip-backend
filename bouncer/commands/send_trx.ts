#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the TRON address provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted in TRX.
//
// For example: ./commands/send_trx.ts TAeUa4FzpoveH7UUArvfXdeS3TmXiWU1ds 1
// will send 1 TRX to account TAeUa4FzpoveH7UUArvfXdeS3TmXiWU1ds
// It also accepts non-encoded hex address representations:
// For example: ./commands/send_trx.ts 0x38a4BCC04f5136e6408589A440F495D7AD0F34DB 1

import { runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';
import { sendTrx } from 'shared/send_trx';

async function main() {
  const tronAddress = process.argv[2];
  const tronAmount = process.argv[3].trim();

  globalLogger.info(`Transferring ${tronAmount} Trx to ${tronAddress}`);

  await sendTrx(globalLogger, tronAddress, tronAmount);
}

await runWithTimeoutAndExit(main(), 20);
