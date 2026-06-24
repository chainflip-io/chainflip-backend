import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple8ImplsCompositeElectionIdentifierExtra,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple8ImplsCompositeElectionIdentifierExtra,
]);

export const ethereumElectionsUnknownElectionEvent = defineEvent(
  'EthereumElections.UnknownElection',
  ethereumElectionsUnknownElection,
);
