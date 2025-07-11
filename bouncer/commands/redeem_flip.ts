#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will redeem Flip from the statechain account defined by the seed in the first argument to
// the Ethereum address provided by the second argument. The amount of redeemed Flip is given by
// the third argument. The asset amount is interpreted as Flip
//
// For example: ./commands/redeem_flip.ts my_secret_seed 0xE16CCFc63368e8FC93f53ccE4e4f4b08c4C3E186 20
// will redeem 20 Flip to from cFPVP4AdJvXoU6cDRGAcEgKRuBtFBE4c1UomJWvdRZ9xBtbwA (derived from "my_secret_seed")
// to 0xE16CCFc63368e8FC93f53ccE4e4f4b08c4C3E186

import { HexString } from '@polkadot/util/types';
import { runWithTimeoutAndExit } from 'shared/utils';
import { RedeemAmount, redeemFlip } from 'shared/redeem_flip';
import { globalLogger } from 'shared/utils/logger';

async function main(): Promise<void> {
  const flipSeed = process.argv[2];
  const ethAddress = process.argv[3] as HexString;
  const flipAmount = process.argv[4].trim();

  const cleanFlipAmount: RedeemAmount =
    flipAmount === 'Max' ? 'Max' : { Exact: flipAmount.toString() };

  await redeemFlip(globalLogger, flipSeed, ethAddress, cleanFlipAmount);
}

await runWithTimeoutAndExit(main(), 600);
