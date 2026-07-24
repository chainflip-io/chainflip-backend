import { arbitrumIngressEgressDepositFinalisedEvent } from '../../arbitrumIngressEgress/depositFinalised';
import { assethubIngressEgressDepositFinalisedEvent } from '../../assethubIngressEgress/depositFinalised';
import { bitcoinIngressEgressDepositFinalisedEvent } from '../../bitcoinIngressEgress/depositFinalised';
import { bscIngressEgressDepositFinalisedEvent } from '../../bscIngressEgress/depositFinalised';
import { ethereumIngressEgressDepositFinalisedEvent } from '../../ethereumIngressEgress/depositFinalised';
import { polkadotIngressEgressDepositFinalisedEvent } from '../../polkadotIngressEgress/depositFinalised';
import { solanaIngressEgressDepositFinalisedEvent } from '../../solanaIngressEgress/depositFinalised';
import { tronIngressEgressDepositFinalisedEvent } from '../../tronIngressEgress/depositFinalised';

export const ingressEgressDepositFinalisedEvent = {
  Arbitrum: arbitrumIngressEgressDepositFinalisedEvent,
  Assethub: assethubIngressEgressDepositFinalisedEvent,
  Bitcoin: bitcoinIngressEgressDepositFinalisedEvent,
  Bsc: bscIngressEgressDepositFinalisedEvent,
  Ethereum: ethereumIngressEgressDepositFinalisedEvent,
  Polkadot: polkadotIngressEgressDepositFinalisedEvent,
  Solana: solanaIngressEgressDepositFinalisedEvent,
  Tron: tronIngressEgressDepositFinalisedEvent,
} as const;
