import { send } from '../shared/send';
import { Asset } from '@chainflip-io/cli';

send(process.argv[2].toUpperCase() as Asset, process.argv[3]);
