import { z } from 'zod';
import {
  hexString,
  palletCfTronIngressEgressFailedVaultDepositsFailedVaultDepositTron,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfTronIngressEgressFailedVaultDepositsFailedVaultDepositTron,
});

export const tronIngressEgressFailedDepositQueuedEvent = defineEvent(
  'TronIngressEgress.FailedDepositQueued',
  tronIngressEgressFailedDepositQueued,
);
