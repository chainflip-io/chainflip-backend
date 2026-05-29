import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});

export const ethereumBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'EthereumBroadcaster.TransactionFeeDeficitRecorded',
  ethereumBroadcasterTransactionFeeDeficitRecorded,
);
