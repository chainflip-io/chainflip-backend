import { z } from 'zod';
import {
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple8ImplsCompositeElectionIdentifierExtra,
} from '../common';

export const ethereumElectionsUnknownElection = z.tuple([
  numberOrHex,
  palletCfElectionsElectoralSystemsCompositeTuple8ImplsCompositeElectionIdentifierExtra,
]);
