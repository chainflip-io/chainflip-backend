import { arbitrumIngressEgressChannelOpeningFeePaidEvent } from '../../arbitrumIngressEgress/channelOpeningFeePaid';
import { assethubIngressEgressChannelOpeningFeePaidEvent } from '../../assethubIngressEgress/channelOpeningFeePaid';
import { bitcoinIngressEgressChannelOpeningFeePaidEvent } from '../../bitcoinIngressEgress/channelOpeningFeePaid';
import { bscIngressEgressChannelOpeningFeePaidEvent } from '../../bscIngressEgress/channelOpeningFeePaid';
import { ethereumIngressEgressChannelOpeningFeePaidEvent } from '../../ethereumIngressEgress/channelOpeningFeePaid';
import { polkadotIngressEgressChannelOpeningFeePaidEvent } from '../../polkadotIngressEgress/channelOpeningFeePaid';
import { solanaIngressEgressChannelOpeningFeePaidEvent } from '../../solanaIngressEgress/channelOpeningFeePaid';
import { tronIngressEgressChannelOpeningFeePaidEvent } from '../../tronIngressEgress/channelOpeningFeePaid';

export const ingressEgressChannelOpeningFeePaidEvent = {
  Arbitrum: arbitrumIngressEgressChannelOpeningFeePaidEvent,
  Assethub: assethubIngressEgressChannelOpeningFeePaidEvent,
  Bitcoin: bitcoinIngressEgressChannelOpeningFeePaidEvent,
  Bsc: bscIngressEgressChannelOpeningFeePaidEvent,
  Ethereum: ethereumIngressEgressChannelOpeningFeePaidEvent,
  Polkadot: polkadotIngressEgressChannelOpeningFeePaidEvent,
  Solana: solanaIngressEgressChannelOpeningFeePaidEvent,
  Tron: tronIngressEgressChannelOpeningFeePaidEvent,
} as const;
