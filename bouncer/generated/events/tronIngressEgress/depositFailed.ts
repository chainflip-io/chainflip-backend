import { z } from 'zod';
import {
  numberOrHex,
  palletCfTronIngressEgressDepositFailedDetailsTron,
  palletCfTronIngressEgressDepositFailedReason,
} from '../common';

export const tronIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfTronIngressEgressDepositFailedReason,
  details: palletCfTronIngressEgressDepositFailedDetailsTron,
});
