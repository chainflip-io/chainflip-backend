import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const bscIngressEgressUnknownBrokerEvent = defineEvent(
  'BscIngressEgress.UnknownBroker',
  bscIngressEgressUnknownBroker,
);
