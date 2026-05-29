import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});

export const polkadotBroadcasterTransactionFeeDeficitRecordedEvent = defineEvent(
  'PolkadotBroadcaster.TransactionFeeDeficitRecorded',
  polkadotBroadcasterTransactionFeeDeficitRecorded,
);
