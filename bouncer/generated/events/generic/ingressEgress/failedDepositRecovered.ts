import { arbitrumIngressEgressFailedDepositRecoveredEvent } from '../../arbitrumIngressEgress/failedDepositRecovered';
import { assethubIngressEgressFailedDepositRecoveredEvent } from '../../assethubIngressEgress/failedDepositRecovered';
import { bitcoinIngressEgressFailedDepositRecoveredEvent } from '../../bitcoinIngressEgress/failedDepositRecovered';
import { bscIngressEgressFailedDepositRecoveredEvent } from '../../bscIngressEgress/failedDepositRecovered';
import { ethereumIngressEgressFailedDepositRecoveredEvent } from '../../ethereumIngressEgress/failedDepositRecovered';
import { polkadotIngressEgressFailedDepositRecoveredEvent } from '../../polkadotIngressEgress/failedDepositRecovered';
import { solanaIngressEgressFailedDepositRecoveredEvent } from '../../solanaIngressEgress/failedDepositRecovered';
import { tronIngressEgressFailedDepositRecoveredEvent } from '../../tronIngressEgress/failedDepositRecovered';

export const ingressEgressFailedDepositRecoveredEvent = {
  Arbitrum: arbitrumIngressEgressFailedDepositRecoveredEvent,
  Assethub: assethubIngressEgressFailedDepositRecoveredEvent,
  Bitcoin: bitcoinIngressEgressFailedDepositRecoveredEvent,
  Bsc: bscIngressEgressFailedDepositRecoveredEvent,
  Ethereum: ethereumIngressEgressFailedDepositRecoveredEvent,
  Polkadot: polkadotIngressEgressFailedDepositRecoveredEvent,
  Solana: solanaIngressEgressFailedDepositRecoveredEvent,
  Tron: tronIngressEgressFailedDepositRecoveredEvent,
} as const;
