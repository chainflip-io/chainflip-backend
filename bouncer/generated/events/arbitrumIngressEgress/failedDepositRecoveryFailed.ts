import { z } from 'zod';
import { accountId, hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressFailedDepositRecoveryFailed = z.object({
  queueId: hexString,
  brokerId: accountId,
  error: spRuntimeDispatchError,
});

export const arbitrumIngressEgressFailedDepositRecoveryFailedEvent = defineEvent(
  'ArbitrumIngressEgress.FailedDepositRecoveryFailed',
  arbitrumIngressEgressFailedDepositRecoveryFailed,
);
