import { z } from 'zod';
import {
  hexString,
  palletCfBitcoinIngressEgressFailedVaultDepositsFailedVaultDepositBitcoin,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressFailedDepositQueued = z.object({
  queueId: hexString,
  deposit: palletCfBitcoinIngressEgressFailedVaultDepositsFailedVaultDepositBitcoin,
});

export const bitcoinIngressEgressFailedDepositQueuedEvent = defineEvent(
  'BitcoinIngressEgress.FailedDepositQueued',
  bitcoinIngressEgressFailedDepositQueued,
);
