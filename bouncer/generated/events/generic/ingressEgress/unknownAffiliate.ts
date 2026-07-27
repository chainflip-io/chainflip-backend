import { arbitrumIngressEgressUnknownAffiliateEvent } from '../../arbitrumIngressEgress/unknownAffiliate';
import { assethubIngressEgressUnknownAffiliateEvent } from '../../assethubIngressEgress/unknownAffiliate';
import { bitcoinIngressEgressUnknownAffiliateEvent } from '../../bitcoinIngressEgress/unknownAffiliate';
import { bscIngressEgressUnknownAffiliateEvent } from '../../bscIngressEgress/unknownAffiliate';
import { ethereumIngressEgressUnknownAffiliateEvent } from '../../ethereumIngressEgress/unknownAffiliate';
import { polkadotIngressEgressUnknownAffiliateEvent } from '../../polkadotIngressEgress/unknownAffiliate';
import { solanaIngressEgressUnknownAffiliateEvent } from '../../solanaIngressEgress/unknownAffiliate';
import { tronIngressEgressUnknownAffiliateEvent } from '../../tronIngressEgress/unknownAffiliate';

export const ingressEgressUnknownAffiliateEvent = {
  Arbitrum: arbitrumIngressEgressUnknownAffiliateEvent,
  Assethub: assethubIngressEgressUnknownAffiliateEvent,
  Bitcoin: bitcoinIngressEgressUnknownAffiliateEvent,
  Bsc: bscIngressEgressUnknownAffiliateEvent,
  Ethereum: ethereumIngressEgressUnknownAffiliateEvent,
  Polkadot: polkadotIngressEgressUnknownAffiliateEvent,
  Solana: solanaIngressEgressUnknownAffiliateEvent,
  Tron: tronIngressEgressUnknownAffiliateEvent,
} as const;
