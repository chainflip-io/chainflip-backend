import { z } from 'zod';
import { accountId, hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressFailedDepositRecoveryFailed = z.object({
  queueId: hexString,
  brokerId: accountId,
  error: spRuntimeDispatchError,
});

export const tronIngressEgressFailedDepositRecoveryFailedEvent = defineEvent(
  'TronIngressEgress.FailedDepositRecoveryFailed',
  tronIngressEgressFailedDepositRecoveryFailed,
);
