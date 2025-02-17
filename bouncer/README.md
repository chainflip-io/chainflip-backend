# chainflip-bouncer

The chainflip-bouncer is a set of end-to-end testing scripts that can be used to
run various scenarios against a deployed chainflip chain. Currently it only supports
localnets.

## Installation / Setup

You need [NodeJS](https://github.com/nvm-sh/nvm#installing-and-updating) and JQ
on your machine:

```sh
brew install jq
```

Then you need to install the dependencies:

```sh
cd bouncer
npm install -g pnpm
pnpm install
```

Note: If npm does not install outdated version of pnpm, you can use corepack to install the latest version:
`corepack prepare pnpm@latest --activate`

Now you can use the provided scripts, assuming that a localnet is already running on your machine.
To connect to a remote network such as a Devnet, you need to set the following environment variables:

```bash
 export CF_NODE_ENDPOINT=
 export POLKADOT_ENDPOINT=
 export BTC_ENDPOINT=
 export ETH_ENDPOINT=
 export BROKER_ENDPOINT=
 ...
```

The values for your network can be found in the `eth-contracts` vault in 1Password.

## Useful commands

The following commands should be executed from the bouncer directory.
All of the checks must find 0 issues to pass the CI.

```sh
# Check formatting
pnpm prettier:check
# Format code
pnpm prettier:write
# Check linting
pnpm eslint:check
# Fix linting
pnpm eslint:fix
# Check Compiler
pnpm tsc --noEmit
```

## How to create bouncer test

### Writing the test

Create a file for your test in the `/tests/` folder.
This file will contain all code related to the test.
The main function to run the test must take the `TestContext` as the first argument.
The `TestContext` contains swap context and a logger that already has the test name attached to it (given in `Running the test` below).

```ts
// bouncer/tests/myTest.ts
export async function myNewTestFunction(testContext: TestContext, seed?: string) {
  /* Test code */
  testContext.debug('example message');
}
```

In summary, your test should:

- Have a function that takes the `TestContext` as the first argument.
- Have all other arguments of that function be optional. (If required, wrap it in a function that uses defaults).
- **Not** be a `.test.ts` file.
- **Not** exit the process. ie, not include `process.exit(0)`.
- only use the given logger. ie do not use any `console.log()`.

### Running the test

To run the test you must add it to one of the test groups in a `.test.ts` file.
Choose the correct location for your test.
This will determine how `vitest` and the CI run it.

| File                   | Test Group        | Description                                                      | ci-development | ci-main-merge |
| ---------------------- | ----------------- | ---------------------------------------------------------------- | -------------- | ------------- |
| `fast_bouncer.test.ts` | `ConcurrentTests` | (Best Option) Tests that can run at the same time                | ✅             | ✅            |
| `full_bouncer.test.ts` | `SerialTests1`    | Tests that must be ran one at a time before the concurrent tests | ❌             | ✅            |
| `full_bouncer.test.ts` | `SerialTests2`    | Tests that must be ran one at a time after the concurrent tests  | ❌             | ✅            |

Using either the `concurrentTest` or `serialTest` function, add the test with with its name, main function (the one that takes `TestContext`) as the timeout in seconds.

```ts
// bouncer/tests/fast_bouncer.test.ts
describe('ConcurrentTests', () => {
  /* .. Other tests .. */
  concurrentTest('myNewTest', myNewTestFunction, 300);
});
```

Now that the test is added, we can run it using the `vitest` command:

```sh copy
pnpm vitest run -t "myNewTest"
```

You can use `vitest`s `list` command to find the name of that test you what to run:

```sh copy
pnpm vitest list
```

If your test uses the `SwapContext` within the `TestContext`, then the report will be automatically logged when the test finishes.
If you would like to run your test with custom arguments, then you will have to create a test command file.
See the `test_commands` folder for examples.

Ways to run multiple test:

```sh
# Run just the tests in a test group
pnpm vitest run -t "ConcurrentTests"

# run all tests in a file
pnpm vitest run -t ./tests/fast_bouncer.test.ts
```

## Logging

In the bouncer we use `Pino` as our logging framework. It has been configured to output the logs to:

- `stdout` at `INFO` level in pretty format
- `/tmp/chainflip/bouncer.log` at `TRACE` level in `JSON` format

Note: To keep the `stdout` clean, that output is configured to ignore the common `test`, `tag` and `module` bindings that ar attached to log messages.

### Logging in a test

Use the `Logger` that is attached to the `TestContext`.
This logger already has the name of the test attached to it.
Do not use `console.log` as it will not be logged to the file.

```ts
import { TestContext } from '../shared/utils/test_context';
import { Logger, throwError } from '../shared/utils/logger';

async function testCase(parentLogger: Logger, asset: Asset) {
  // Attach any contextual data to the logger as a binding. {"inputAsset": "Eth"}.
  const logger = parentLogger.child({ inputAsset: asset });

  // Basic logging
  logger.debug('About to foo');

  // Throwing an error with all of the loggers contextual data (bindings) added to the error message.
  // No need to catch this, it will be logged as at `Error` level.
  if (foo) {
    throwError(logger, new Error('Foo happened'));
  }
}

export async function myTest(testContext: TestContext) {
  await testCase(testContext.logger, 'Eth');

  // You can log directly from the `TestContext` as well
  testContext.info('Goodbye');
}
```

Another option for adding information to the logger is using the custom `loggerChild` function.
This will append the given string to the `module` binding.

```ts
import { loggerChild } from '../shared/utils/logger';

const logger1 = loggerChild(parentLogger, `myTestCase`);
const logger2 = loggerChild(logger1, `setupFunction`);
const logger3 = loggerChild(logger2, `myFunction`);
// {"module": `myTestCase::setupFunction::myFunction`}
```

### Logging outside of a test

If you need use the logger in a command or any other non-test code, you can use the `globalLogger`.
It has no bindings attached to it.
It will still output to both `stdout` and the `bouncer.log` file.

```ts
import { globalLogger } from '../shared/utils/logger';

globalLogger.info('Executing my command');
await observeEvent(globalLogger, 'someEvent');
```

### Debugging

To debug a failed test you can use the `bouncer.log` file. It will have logs at `Trace` level of all test that where ran.
To filter for the test you are debugging, you can use the `test` value that should be on all log messages (Excluding logs from commands, setup scripts and thrown errors).

An example of using `jq` to filter the logs for an individual test and put them in another file:

```sh copy
jq 'select(.test=="BoostingForAsset")' /tmp/chainflip/bouncer.log > /tmp/chainflip/failed_test.log
```

If you want to run a test and have the `stdout` logs be at different level, you can use the `BOUNCER_LOG_LEVEL` environment variable (Does not effect the `bouncer.log` file).

```sh copy
BOUNCER_LOG_LEVEL=debug pnpm vitest run -t "BoostingForAsset"
```

Note: you can use the `BOUNCER_LOG_PATH` environment variable to output the logs to a different file.
