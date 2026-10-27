import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentFailedDepositRecovered = z.object({
  broker: accountId,
  recoveryAccount: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
  reference: hexString,
});

export const environmentFailedDepositRecoveredEvent = defineEvent(
  'Environment.FailedDepositRecovered',
  environmentFailedDepositRecovered,
);
