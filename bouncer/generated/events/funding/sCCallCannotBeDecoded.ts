import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingSCCallCannotBeDecoded = z.object({
  caller: accountId,
  scCallBytes: hexString,
  ethTxHash: hexString,
});

export const fundingSCCallCannotBeDecodedEvent = defineEvent(
  'Funding.SCCallCannotBeDecoded',
  fundingSCCallCannotBeDecoded,
);
