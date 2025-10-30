#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will fund and register an account as LP
//
// For example: ./commands/fund_lp_account.ts //LP_3

import { runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';
import { assetConstants } from '@chainflip/cli';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { setupLpAccount } from 'shared/setup_account';
import { z } from 'zod';

const args = z.tuple([
  z.any(),
  z.any(),
  z.string().refine((val) => ['uri', 'mnemonic', 'evm'].includes(val), {
    message: 'Key type must be "uri" or "mnemonic" or "evm"',
  }),
  z
    .string()
    .transform((val) => JSON.parse(val))
    .refine((val) => Array.isArray(val) && val.length > 0, { message: 'LP keys must be provided' }),
]);

async function main() {
  const [_, __, keyType, lpKeys] = args.parse(process.argv);

  for (const key of lpKeys) {
    await setupLpAccount(globalLogger, key);

    for (const asset of Object.keys(assetConstants).filter((asset) =>
      [
        'Btc',
        'Eth',
        'Usdc',
        // 'Usdt', Throwing some weird errors..
        'Sol',
      ].includes(asset),
    )) {
      let amount;
      switch (asset) {
        case 'Btc':
          amount = 2;
          break;
        case 'Eth':
          amount = 10;
          break;
        case 'Usdc':
          amount = 10000;
          break;
        case 'Usdt':
          amount = 10000;
          break;
        case 'Sol':
          amount = 10;
          break;
        default:
          amount = 1000;
          break;
      }

      amount = lpKeys.length == 1 ? amount * 10000 : amount;

      await depositLiquidity(
        globalLogger,
        asset as any,
        amount,
        false,
        keyType === 'uri' ? key : undefined,
        keyType === 'mnemonic' ? key : undefined,
      );
    }
  }
}

await runWithTimeoutAndExit(main(), 120_000);
