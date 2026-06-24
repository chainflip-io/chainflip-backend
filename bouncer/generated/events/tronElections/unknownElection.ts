import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple5ImplsCompositeElectionIdentifierExtra,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple5ImplsCompositeElectionIdentifierExtra,
]);

export const tronElectionsUnknownElectionEvent = defineEvent(
  'TronElections.UnknownElection',
  tronElectionsUnknownElection,
);
