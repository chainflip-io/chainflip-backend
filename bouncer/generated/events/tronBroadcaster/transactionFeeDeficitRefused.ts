import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });

export const tronBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'TronBroadcaster.TransactionFeeDeficitRefused',
  tronBroadcasterTransactionFeeDeficitRefused,
);
