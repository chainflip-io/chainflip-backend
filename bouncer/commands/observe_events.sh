#!/usr/bin/env node

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
// For example: ./commands/observe_events.sh --succeed_on ethereumThresholdSigner:ThresholdSignatureSuccess --fail_on ethereumThresholdSigner:SignersUnavailable

const { ApiPromise, WsProvider } = require('@polkadot/api');
const args = require('minimist')(process.argv.slice(2));
const { runWithTimeout } = require('../shared/utils');

async function main() {
	var cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	var expected_events = args.succeed_on.split(",");
	const print_event = expected_events.length == 1;
	const bad_events = args.fail_on.split(",");
	const api = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});
	api.query.system.events((events) => {
		events.forEach((record) => {
			const {event, phase} = record;
			bad_events.forEach((bad_event_iterator) => {
				const bad_event = bad_event_iterator.split(":");
				if(event.section === bad_event[0] && event.method === bad_event[1]){
				console.log("Found event " + bad_event_iterator);
				process.exit(-1);
			}});
			for(let i=0; i<expected_events.length; i++){
				const expected_event = expected_events[i].split(":");
				if(event.section === expected_event[0] && event.method === expected_event[1]){
					if(print_event) {
						console.log(event.data.toString());
					}
					// remove the expected event from the list
					expected_events.splice(i, 1);
					break;
				}
			}
			if(expected_events.length == 0){
				process.exit(0);
			}
		});
	});
}

runWithTimeout(main(), args.timeout || 10000).catch((error) => {
	console.error(error);
	process.exit(-1);
});
