import { z } from 'zod';
import {
  palletCfAssethubIngressEgressDepositFailedDetailsAssethub,
  palletCfAssethubIngressEgressDepositFailedReason,
} from '../common';

export const assethubIngressEgressDepositFailed = z.object({
  blockHeight: z.number(),
  reason: palletCfAssethubIngressEgressDepositFailedReason,
  details: palletCfAssethubIngressEgressDepositFailedDetailsAssethub,
});
