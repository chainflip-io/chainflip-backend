import { z } from 'zod';
import { accountId, hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressFailedDepositRecoveryFailed = z.object({
  queueId: hexString,
  brokerId: accountId,
  error: spRuntimeDispatchError,
});

export const bitcoinIngressEgressFailedDepositRecoveryFailedEvent = defineEvent(
  'BitcoinIngressEgress.FailedDepositRecoveryFailed',
  bitcoinIngressEgressFailedDepositRecoveryFailed,
);
