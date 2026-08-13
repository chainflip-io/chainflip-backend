import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple5ImplsCompositeElectionIdentifierExtra,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple5ImplsCompositeElectionIdentifierExtra,
]);

export const assethubElectionsUnknownElectionEvent = defineEvent(
  'AssethubElections.UnknownElection',
  assethubElectionsUnknownElection,
);
