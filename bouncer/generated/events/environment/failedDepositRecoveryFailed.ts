import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  hexString,
  numberOrHex,
  spRuntimeDispatchError,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentFailedDepositRecoveryFailed = z.object({
  broker: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  reference: hexString,
  error: spRuntimeDispatchError,
});

export const environmentFailedDepositRecoveryFailedEvent = defineEvent(
  'Environment.FailedDepositRecoveryFailed',
  environmentFailedDepositRecoveryFailed,
);
