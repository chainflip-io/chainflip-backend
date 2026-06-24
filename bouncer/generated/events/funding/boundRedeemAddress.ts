import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingBoundRedeemAddress = z.object({ accountId, address: hexString });

export const fundingBoundRedeemAddressEvent = defineEvent(
  'Funding.BoundRedeemAddress',
  fundingBoundRedeemAddress,
);
