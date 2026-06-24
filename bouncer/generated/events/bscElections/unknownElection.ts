import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple6ImplsCompositeElectionIdentifierExtra,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple6ImplsCompositeElectionIdentifierExtra,
]);

export const bscElectionsUnknownElectionEvent = defineEvent(
  'BscElections.UnknownElection',
  bscElectionsUnknownElection,
);
