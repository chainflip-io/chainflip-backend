import { z } from 'zod';
import {
  palletCfAssethubIngressEgressDepositFailedDetailsAssethub,
  palletCfAssethubIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressDepositFailed = z.object({
  blockHeight: z.number(),
  reason: palletCfAssethubIngressEgressDepositFailedReason,
  details: palletCfAssethubIngressEgressDepositFailedDetailsAssethub,
});

export const assethubIngressEgressDepositFailedEvent = defineEvent(
  'AssethubIngressEgress.DepositFailed',
  assethubIngressEgressDepositFailed,
);
