import { z } from 'zod';
import { cfPrimitivesP2pTransport } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentP2pTransportUpdated = z.object({ transport: cfPrimitivesP2pTransport });

export const environmentP2pTransportUpdatedEvent = defineEvent(
  'Environment.P2pTransportUpdated',
  environmentP2pTransportUpdated,
);
