import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const assethubIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'AssethubIngressEgress.ChannelOpeningFeePaid',
  assethubIngressEgressChannelOpeningFeePaid,
);
