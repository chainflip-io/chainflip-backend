import { arbitrumIngressEgressTransactionRejectionFailedEvent } from '../../arbitrumIngressEgress/transactionRejectionFailed';
import { assethubIngressEgressTransactionRejectionFailedEvent } from '../../assethubIngressEgress/transactionRejectionFailed';
import { bitcoinIngressEgressTransactionRejectionFailedEvent } from '../../bitcoinIngressEgress/transactionRejectionFailed';
import { bscIngressEgressTransactionRejectionFailedEvent } from '../../bscIngressEgress/transactionRejectionFailed';
import { ethereumIngressEgressTransactionRejectionFailedEvent } from '../../ethereumIngressEgress/transactionRejectionFailed';
import { polkadotIngressEgressTransactionRejectionFailedEvent } from '../../polkadotIngressEgress/transactionRejectionFailed';
import { solanaIngressEgressTransactionRejectionFailedEvent } from '../../solanaIngressEgress/transactionRejectionFailed';
import { tronIngressEgressTransactionRejectionFailedEvent } from '../../tronIngressEgress/transactionRejectionFailed';

export const ingressEgressTransactionRejectionFailedEvent = {
  Arbitrum: arbitrumIngressEgressTransactionRejectionFailedEvent,
  Assethub: assethubIngressEgressTransactionRejectionFailedEvent,
  Bitcoin: bitcoinIngressEgressTransactionRejectionFailedEvent,
  Bsc: bscIngressEgressTransactionRejectionFailedEvent,
  Ethereum: ethereumIngressEgressTransactionRejectionFailedEvent,
  Polkadot: polkadotIngressEgressTransactionRejectionFailedEvent,
  Solana: solanaIngressEgressTransactionRejectionFailedEvent,
  Tron: tronIngressEgressTransactionRejectionFailedEvent,
} as const;
