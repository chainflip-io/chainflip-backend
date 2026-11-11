import { z } from 'zod';
import { accountId, hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressFailedDepositRecoveryFailed = z.object({
  queueId: hexString,
  brokerId: accountId,
  error: spRuntimeDispatchError,
});

export const bscIngressEgressFailedDepositRecoveryFailedEvent = defineEvent(
  'BscIngressEgress.FailedDepositRecoveryFailed',
  bscIngressEgressFailedDepositRecoveryFailed,
);
