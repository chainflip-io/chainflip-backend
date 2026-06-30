import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const bscIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'BscIngressEgress.ChannelOpeningFeePaid',
  bscIngressEgressChannelOpeningFeePaid,
);
