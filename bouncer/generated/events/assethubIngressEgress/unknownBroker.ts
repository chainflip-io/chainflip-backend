import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const assethubIngressEgressUnknownBrokerEvent = defineEvent(
  'AssethubIngressEgress.UnknownBroker',
  assethubIngressEgressUnknownBroker,
);
