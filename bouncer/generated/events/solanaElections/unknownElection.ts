import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple7ImplsCompositeElectionIdentifierExtra,
} from '../common';

export const solanaElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple7ImplsCompositeElectionIdentifierExtra,
]);
