import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple7ImplsCompositeElectionIdentifierExtra,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple7ImplsCompositeElectionIdentifierExtra,
]);

export const solanaElectionsUnknownElectionEvent = defineEvent(
  'SolanaElections.UnknownElection',
  solanaElectionsUnknownElection,
);
