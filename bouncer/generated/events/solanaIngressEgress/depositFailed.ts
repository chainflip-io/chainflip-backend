import { z } from 'zod';
import {
  numberOrHex,
  palletCfSolanaIngressEgressDepositFailedDetailsSolana,
  palletCfSolanaIngressEgressDepositFailedReason,
} from '../common';

export const solanaIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfSolanaIngressEgressDepositFailedReason,
  details: palletCfSolanaIngressEgressDepositFailedDetailsSolana,
});
