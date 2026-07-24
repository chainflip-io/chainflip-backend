import { arbitrumIngressEgressTransactionRejectionRequestExpiredEvent } from '../../arbitrumIngressEgress/transactionRejectionRequestExpired';
import { assethubIngressEgressTransactionRejectionRequestExpiredEvent } from '../../assethubIngressEgress/transactionRejectionRequestExpired';
import { bitcoinIngressEgressTransactionRejectionRequestExpiredEvent } from '../../bitcoinIngressEgress/transactionRejectionRequestExpired';
import { bscIngressEgressTransactionRejectionRequestExpiredEvent } from '../../bscIngressEgress/transactionRejectionRequestExpired';
import { ethereumIngressEgressTransactionRejectionRequestExpiredEvent } from '../../ethereumIngressEgress/transactionRejectionRequestExpired';
import { polkadotIngressEgressTransactionRejectionRequestExpiredEvent } from '../../polkadotIngressEgress/transactionRejectionRequestExpired';
import { solanaIngressEgressTransactionRejectionRequestExpiredEvent } from '../../solanaIngressEgress/transactionRejectionRequestExpired';
import { tronIngressEgressTransactionRejectionRequestExpiredEvent } from '../../tronIngressEgress/transactionRejectionRequestExpired';

export const ingressEgressTransactionRejectionRequestExpiredEvent = {
  Arbitrum: arbitrumIngressEgressTransactionRejectionRequestExpiredEvent,
  Assethub: assethubIngressEgressTransactionRejectionRequestExpiredEvent,
  Bitcoin: bitcoinIngressEgressTransactionRejectionRequestExpiredEvent,
  Bsc: bscIngressEgressTransactionRejectionRequestExpiredEvent,
  Ethereum: ethereumIngressEgressTransactionRejectionRequestExpiredEvent,
  Polkadot: polkadotIngressEgressTransactionRejectionRequestExpiredEvent,
  Solana: solanaIngressEgressTransactionRejectionRequestExpiredEvent,
  Tron: tronIngressEgressTransactionRejectionRequestExpiredEvent,
} as const;
