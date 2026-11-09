import { z } from 'zod';
import {
  hexString,
  palletCfSolanaIngressEgressFailedVaultDepositsFailedVaultDepositSolana,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfSolanaIngressEgressFailedVaultDepositsFailedVaultDepositSolana,
});

export const solanaIngressEgressFailedDepositQueuedEvent = defineEvent(
  'SolanaIngressEgress.FailedDepositQueued',
  solanaIngressEgressFailedDepositQueued,
);
