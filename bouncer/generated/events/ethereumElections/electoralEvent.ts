import { z } from 'zod';
import { stateChainRuntimeChainflipWitnessingEthereumElectionsEthereumElectoralEvents } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumElectionsElectoralEvent =
  stateChainRuntimeChainflipWitnessingEthereumElectionsEthereumElectoralEvents;

export const ethereumElectionsElectoralEventEvent = defineEvent(
  'EthereumElections.ElectoralEvent',
  ethereumElectionsElectoralEvent,
);
