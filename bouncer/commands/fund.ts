import { Asset } from '@chainflip-io/cli/.';
import { fund } from '../shared/fund';

fund(process.argv[2].toUpperCase() as Asset, process.argv[3]);