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

| File                   | Test Group        | Description                                                  | ci-development | ci-main-merge |
| ---------------------- | ----------------- | ------------------------------------------------------------ | -------------- | ------------- |
| `fast_bouncer.test.ts` | `ConcurrentTests` | (Best Option) Tests that can run at the same time            | ✅             | ✅            |
| `fast_bouncer.test.ts` | `SerialTests`     | Tests that must be ran one at a time                         | ✅             | ✅            |
| `full_bouncer.test.ts` | `SlowTests`       | Low priority / low risk tests that must be ran one at a time | ❌             | ✅            |

Using either the `concurrentTest` or `serialTest` function, add the test with with its name, main function (the one that takes `TestContext`) as the timeout in seconds.

```ts
// bouncer/tests/fast_bouncer.test.ts
describe('ConcurrentTests', () => {
  /* .. Other tests .. */
  concurrentTest('myNewTest', myNewTestFunction, 300);
});
```

Now that the test is added, we can run it using the `vitest` command.

```sh copy
pnpm vitest run -t "myNewTest"
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
