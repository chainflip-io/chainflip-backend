import { z } from 'zod';
import { cfChainsBtcScriptPubkey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterTransactionFeeDeficitRefused = z.object({
  beneficiary: cfChainsBtcScriptPubkey,
});

export const bitcoinBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'BitcoinBroadcaster.TransactionFeeDeficitRefused',
  bitcoinBroadcasterTransactionFeeDeficitRefused,
);
