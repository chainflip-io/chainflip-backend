import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfTronIngressEgressFailedVaultDepositsFailedVaultDepositInfoTron,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfTronIngressEgressFailedVaultDepositsFailedVaultDepositInfoTron,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const tronIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'TronIngressEgress.FailedDepositRecovered',
  tronIngressEgressFailedDepositRecovered,
);
