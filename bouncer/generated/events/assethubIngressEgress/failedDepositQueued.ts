import { z } from 'zod';
import {
  hexString,
  palletCfAssethubIngressEgressFailedVaultDepositsFailedVaultDepositAssethub,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfAssethubIngressEgressFailedVaultDepositsFailedVaultDepositAssethub,
});

export const assethubIngressEgressFailedDepositQueuedEvent = defineEvent(
  'AssethubIngressEgress.FailedDepositQueued',
  assethubIngressEgressFailedDepositQueued,
);
