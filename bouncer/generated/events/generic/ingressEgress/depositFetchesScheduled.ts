import { arbitrumIngressEgressDepositFetchesScheduledEvent } from '../../arbitrumIngressEgress/depositFetchesScheduled';
import { assethubIngressEgressDepositFetchesScheduledEvent } from '../../assethubIngressEgress/depositFetchesScheduled';
import { bitcoinIngressEgressDepositFetchesScheduledEvent } from '../../bitcoinIngressEgress/depositFetchesScheduled';
import { bscIngressEgressDepositFetchesScheduledEvent } from '../../bscIngressEgress/depositFetchesScheduled';
import { ethereumIngressEgressDepositFetchesScheduledEvent } from '../../ethereumIngressEgress/depositFetchesScheduled';
import { polkadotIngressEgressDepositFetchesScheduledEvent } from '../../polkadotIngressEgress/depositFetchesScheduled';
import { solanaIngressEgressDepositFetchesScheduledEvent } from '../../solanaIngressEgress/depositFetchesScheduled';
import { tronIngressEgressDepositFetchesScheduledEvent } from '../../tronIngressEgress/depositFetchesScheduled';

export const ingressEgressDepositFetchesScheduledEvent = {
  Arbitrum: arbitrumIngressEgressDepositFetchesScheduledEvent,
  Assethub: assethubIngressEgressDepositFetchesScheduledEvent,
  Bitcoin: bitcoinIngressEgressDepositFetchesScheduledEvent,
  Bsc: bscIngressEgressDepositFetchesScheduledEvent,
  Ethereum: ethereumIngressEgressDepositFetchesScheduledEvent,
  Polkadot: polkadotIngressEgressDepositFetchesScheduledEvent,
  Solana: solanaIngressEgressDepositFetchesScheduledEvent,
  Tron: tronIngressEgressDepositFetchesScheduledEvent,
} as const;
