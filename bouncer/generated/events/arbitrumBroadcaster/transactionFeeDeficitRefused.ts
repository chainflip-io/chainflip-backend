import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });

export const arbitrumBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'ArbitrumBroadcaster.TransactionFeeDeficitRefused',
  arbitrumBroadcasterTransactionFeeDeficitRefused,
);
