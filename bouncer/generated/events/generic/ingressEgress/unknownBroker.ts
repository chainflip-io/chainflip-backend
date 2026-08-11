import { arbitrumIngressEgressUnknownBrokerEvent } from '../../arbitrumIngressEgress/unknownBroker';
import { assethubIngressEgressUnknownBrokerEvent } from '../../assethubIngressEgress/unknownBroker';
import { bitcoinIngressEgressUnknownBrokerEvent } from '../../bitcoinIngressEgress/unknownBroker';
import { bscIngressEgressUnknownBrokerEvent } from '../../bscIngressEgress/unknownBroker';
import { ethereumIngressEgressUnknownBrokerEvent } from '../../ethereumIngressEgress/unknownBroker';
import { polkadotIngressEgressUnknownBrokerEvent } from '../../polkadotIngressEgress/unknownBroker';
import { solanaIngressEgressUnknownBrokerEvent } from '../../solanaIngressEgress/unknownBroker';
import { tronIngressEgressUnknownBrokerEvent } from '../../tronIngressEgress/unknownBroker';

export const ingressEgressUnknownBrokerEvent = {
  Arbitrum: arbitrumIngressEgressUnknownBrokerEvent,
  Assethub: assethubIngressEgressUnknownBrokerEvent,
  Bitcoin: bitcoinIngressEgressUnknownBrokerEvent,
  Bsc: bscIngressEgressUnknownBrokerEvent,
  Ethereum: ethereumIngressEgressUnknownBrokerEvent,
  Polkadot: polkadotIngressEgressUnknownBrokerEvent,
  Solana: solanaIngressEgressUnknownBrokerEvent,
  Tron: tronIngressEgressUnknownBrokerEvent,
} as const;
