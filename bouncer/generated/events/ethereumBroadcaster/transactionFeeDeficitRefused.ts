import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });

export const ethereumBroadcasterTransactionFeeDeficitRefusedEvent = defineEvent(
  'EthereumBroadcaster.TransactionFeeDeficitRefused',
  ethereumBroadcasterTransactionFeeDeficitRefused,
);
