import { z } from 'zod';
import { accountId, hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressFailedDepositRecoveryFailed = z.object({
  queueId: hexString,
  brokerId: accountId,
  error: spRuntimeDispatchError,
});

export const assethubIngressEgressFailedDepositRecoveryFailedEvent = defineEvent(
  'AssethubIngressEgress.FailedDepositRecoveryFailed',
  assethubIngressEgressFailedDepositRecoveryFailed,
);
