import { z } from 'zod';
import {
  hexString,
  palletCfEthereumIngressEgressFailedVaultDepositsFailedVaultDepositEthereum,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfEthereumIngressEgressFailedVaultDepositsFailedVaultDepositEthereum,
});

export const ethereumIngressEgressFailedDepositQueuedEvent = defineEvent(
  'EthereumIngressEgress.FailedDepositQueued',
  ethereumIngressEgressFailedDepositQueued,
);
