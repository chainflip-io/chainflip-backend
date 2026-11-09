import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfArbitrumIngressEgressFailedVaultDepositsFailedVaultDepositInfoArbitrum,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfArbitrumIngressEgressFailedVaultDepositsFailedVaultDepositInfoArbitrum,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const arbitrumIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'ArbitrumIngressEgress.FailedDepositRecovered',
  arbitrumIngressEgressFailedDepositRecovered,
);
