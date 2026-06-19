import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingBoundExecutorAddress = z.object({ accountId, address: hexString });

export const fundingBoundExecutorAddressEvent = defineEvent(
  'Funding.BoundExecutorAddress',
  fundingBoundExecutorAddress,
);
