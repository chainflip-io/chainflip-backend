import { z } from 'zod';
import {
  numberOrHex,
  palletCfArbitrumIngressEgressDepositFailedDetailsArbitrum,
  palletCfArbitrumIngressEgressDepositFailedReason,
} from '../common';

export const arbitrumIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfArbitrumIngressEgressDepositFailedReason,
  details: palletCfArbitrumIngressEgressDepositFailedDetailsArbitrum,
});
