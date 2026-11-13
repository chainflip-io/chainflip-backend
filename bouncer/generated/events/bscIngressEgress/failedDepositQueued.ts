import { z } from 'zod';
import {
  hexString,
  palletCfBscIngressEgressFailedVaultDepositsFailedVaultDepositBsc,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfBscIngressEgressFailedVaultDepositsFailedVaultDepositBsc,
});

export const bscIngressEgressFailedDepositQueuedEvent = defineEvent(
  'BscIngressEgress.FailedDepositQueued',
  bscIngressEgressFailedDepositQueued,
);
