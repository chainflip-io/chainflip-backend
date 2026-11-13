import { z } from 'zod';
import {
  hexString,
  palletCfArbitrumIngressEgressFailedVaultDepositsFailedVaultDepositArbitrum,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfArbitrumIngressEgressFailedVaultDepositsFailedVaultDepositArbitrum,
});

export const arbitrumIngressEgressFailedDepositQueuedEvent = defineEvent(
  'ArbitrumIngressEgress.FailedDepositQueued',
  arbitrumIngressEgressFailedDepositQueued,
);
