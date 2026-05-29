import { z } from 'zod';
import { cfChainsBtcUtxo } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressTransactionRejectionFailed = z.object({ txId: cfChainsBtcUtxo });

export const bitcoinIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'BitcoinIngressEgress.TransactionRejectionFailed',
  bitcoinIngressEgressTransactionRejectionFailed,
);
