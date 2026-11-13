import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfSolanaIngressEgressFailedVaultDepositsFailedVaultDepositInfoSolana,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfSolanaIngressEgressFailedVaultDepositsFailedVaultDepositInfoSolana,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const solanaIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'SolanaIngressEgress.FailedDepositRecovered',
  solanaIngressEgressFailedDepositRecovered,
);
