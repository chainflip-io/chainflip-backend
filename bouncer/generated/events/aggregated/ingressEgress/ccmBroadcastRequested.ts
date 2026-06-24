import { arbitrumIngressEgressCcmBroadcastRequestedEvent } from '../../arbitrumIngressEgress/ccmBroadcastRequested';
import { assethubIngressEgressCcmBroadcastRequestedEvent } from '../../assethubIngressEgress/ccmBroadcastRequested';
import { bitcoinIngressEgressCcmBroadcastRequestedEvent } from '../../bitcoinIngressEgress/ccmBroadcastRequested';
import { bscIngressEgressCcmBroadcastRequestedEvent } from '../../bscIngressEgress/ccmBroadcastRequested';
import { ethereumIngressEgressCcmBroadcastRequestedEvent } from '../../ethereumIngressEgress/ccmBroadcastRequested';
import { polkadotIngressEgressCcmBroadcastRequestedEvent } from '../../polkadotIngressEgress/ccmBroadcastRequested';
import { solanaIngressEgressCcmBroadcastRequestedEvent } from '../../solanaIngressEgress/ccmBroadcastRequested';
import { tronIngressEgressCcmBroadcastRequestedEvent } from '../../tronIngressEgress/ccmBroadcastRequested';

export const ingressEgressCcmBroadcastRequestedEvent = {
  Arbitrum: arbitrumIngressEgressCcmBroadcastRequestedEvent,
  Assethub: assethubIngressEgressCcmBroadcastRequestedEvent,
  Bitcoin: bitcoinIngressEgressCcmBroadcastRequestedEvent,
  Bsc: bscIngressEgressCcmBroadcastRequestedEvent,
  Ethereum: ethereumIngressEgressCcmBroadcastRequestedEvent,
  Polkadot: polkadotIngressEgressCcmBroadcastRequestedEvent,
  Solana: solanaIngressEgressCcmBroadcastRequestedEvent,
  Tron: tronIngressEgressCcmBroadcastRequestedEvent,
} as const;
