import { z } from 'zod';
import {
  numberOrHex,
  palletCfSolanaIngressEgressDepositFailedDetailsSolana,
  palletCfSolanaIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfSolanaIngressEgressDepositFailedReason,
  details: palletCfSolanaIngressEgressDepositFailedDetailsSolana,
});

export const solanaIngressEgressDepositFailedEvent = defineEvent(
  'SolanaIngressEgress.DepositFailed',
  solanaIngressEgressDepositFailed,
);
