// INSTRUCTIONS
//
// Arguments:
// timeout - (optional, default=10000) The command will fail after this many milliseconds
// succeed_on - If ALL of the provided events are observed, the command will succeed. Events are
//              separated by commas
// fail_on - If ANY of the provided events is observed, the command will fail
//
// This command will monitor the chainflip state-chain for the events provided.
// If only a single event is listed as the succeed_on parameter, the event data will be printed
// to stdout when it is observed
//
// For example: pnpm tsx ./commands/observe_events.ts --succeed_on ethereumThresholdSigner:ThresholdSignatureSuccess --fail_on ethereumThresholdSigner:SignersUnavailable

import { ApiPromise, WsProvider } from '@polkadot/api';
import minimist from 'minimist';
import { runWithTimeout } from '../shared/utils';

const args = minimist(process.argv.slice(2));

async function main(): Promise<void> {

  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  const expectedEvents = args.succeed_on.split(',');
  const printEvent = expectedEvents.length === 1;
  const badEvents = args.fail_on ? args.fail_on.split(',') : [];
  const api = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await api.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;
      badEvents.forEach((badEventIterator: string) => {
        const badEvent = badEventIterator.split(':');
        if (event.section === badEvent[0] && event.method === badEvent[1]) {
          console.log('Found event ' + badEventIterator);
          process.exit(-1);
        }
      });
      for (let i = 0; i < expectedEvents.length; i++) {
        const expectedEvent = expectedEvents[i].split(':');
        if (event.section === expectedEvent[0] && event.method === expectedEvent[1]) {
          if (printEvent) {
            console.log(event.data.toString());
          }
          // remove the expected event from the list
          expectedEvents.splice(i, 1);
          break;
        }
      }
      if (expectedEvents.length === 0) {
        process.exit(0);
      }
    });
  });
}

runWithTimeout(main(), args.timeout ?? 10000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
