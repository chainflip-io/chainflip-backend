import { arbitrumIngressEgressChannelRejectionRequestReceivedEvent } from '../../arbitrumIngressEgress/channelRejectionRequestReceived';
import { assethubIngressEgressChannelRejectionRequestReceivedEvent } from '../../assethubIngressEgress/channelRejectionRequestReceived';
import { bitcoinIngressEgressChannelRejectionRequestReceivedEvent } from '../../bitcoinIngressEgress/channelRejectionRequestReceived';
import { bscIngressEgressChannelRejectionRequestReceivedEvent } from '../../bscIngressEgress/channelRejectionRequestReceived';
import { ethereumIngressEgressChannelRejectionRequestReceivedEvent } from '../../ethereumIngressEgress/channelRejectionRequestReceived';
import { polkadotIngressEgressChannelRejectionRequestReceivedEvent } from '../../polkadotIngressEgress/channelRejectionRequestReceived';
import { solanaIngressEgressChannelRejectionRequestReceivedEvent } from '../../solanaIngressEgress/channelRejectionRequestReceived';
import { tronIngressEgressChannelRejectionRequestReceivedEvent } from '../../tronIngressEgress/channelRejectionRequestReceived';

export const ingressEgressChannelRejectionRequestReceivedEvent = {
  Arbitrum: arbitrumIngressEgressChannelRejectionRequestReceivedEvent,
  Assethub: assethubIngressEgressChannelRejectionRequestReceivedEvent,
  Bitcoin: bitcoinIngressEgressChannelRejectionRequestReceivedEvent,
  Bsc: bscIngressEgressChannelRejectionRequestReceivedEvent,
  Ethereum: ethereumIngressEgressChannelRejectionRequestReceivedEvent,
  Polkadot: polkadotIngressEgressChannelRejectionRequestReceivedEvent,
  Solana: solanaIngressEgressChannelRejectionRequestReceivedEvent,
  Tron: tronIngressEgressChannelRejectionRequestReceivedEvent,
} as const;
