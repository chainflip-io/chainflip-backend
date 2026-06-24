import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const bitcoinIngressEgressUnknownBrokerEvent = defineEvent(
  'BitcoinIngressEgress.UnknownBroker',
  bitcoinIngressEgressUnknownBroker,
);
