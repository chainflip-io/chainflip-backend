import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple1ImplsCompositeElectionIdentifierExtra,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const genericElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple1ImplsCompositeElectionIdentifierExtra,
]);

export const genericElectionsUnknownElectionEvent = defineEvent(
  'GenericElections.UnknownElection',
  genericElectionsUnknownElection,
);
