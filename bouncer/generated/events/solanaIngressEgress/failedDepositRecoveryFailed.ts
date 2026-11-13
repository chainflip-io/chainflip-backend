import { z } from 'zod';
import { accountId, hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressFailedDepositRecoveryFailed = z.object({
  queueId: hexString,
  brokerId: accountId,
  error: spRuntimeDispatchError,
});

export const solanaIngressEgressFailedDepositRecoveryFailedEvent = defineEvent(
  'SolanaIngressEgress.FailedDepositRecoveryFailed',
  solanaIngressEgressFailedDepositRecoveryFailed,
);
