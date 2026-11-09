import { z } from 'zod';
import {
  hexString,
  palletCfPolkadotIngressEgressFailedVaultDepositsFailedVaultDepositPolkadot,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfPolkadotIngressEgressFailedVaultDepositsFailedVaultDepositPolkadot,
});

export const polkadotIngressEgressFailedDepositQueuedEvent = defineEvent(
  'PolkadotIngressEgress.FailedDepositQueued',
  polkadotIngressEgressFailedDepositQueued,
);
