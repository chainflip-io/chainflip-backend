import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const tronIngressEgressUnknownBrokerEvent = defineEvent(
  'TronIngressEgress.UnknownBroker',
  tronIngressEgressUnknownBroker,
);
