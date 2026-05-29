import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple6ImplsCompositeElectionIdentifierExtra,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple6ImplsCompositeElectionIdentifierExtra,
]);

export const arbitrumElectionsUnknownElectionEvent = defineEvent(
  'ArbitrumElections.UnknownElection',
  arbitrumElectionsUnknownElection,
);
