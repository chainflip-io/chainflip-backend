import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const emissionsNetworkFeeBurned = z.object({
  amount: numberOrHex,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
});

export const emissionsNetworkFeeBurnedEvent = defineEvent(
  'Emissions.NetworkFeeBurned',
  emissionsNetworkFeeBurned,
);
