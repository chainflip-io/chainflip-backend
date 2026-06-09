import prisma from 'shared/utils/prisma_client';
import { TestContext } from 'shared/utils/test_context';

// Whenever one of the engine's witnessing tasks crashes (panics or returns an unexpected error) it
// is restarted by `spawn_with_restart`, and the validator submits the "SOS extrinsic"
// (`validator.report_witnessing_task_restart`, see `submit_sos_extrinsic` in the engine) which
// emits the `Validator.WitnessingTaskRestarted` event.
//
// A crash like this is always critical: it usually means some witnessing/voting code panicked,
// including code that runs behind an RPC. The state chain keeps running when this happens, so
// without this check the bouncer would happily pass even though a panic occurred. We make a single
// query against the indexer to assert that no such event was emitted during the test run.
const WITNESSING_TASK_RESTARTED_EVENT = 'Validator.WitnessingTaskRestarted';

export async function checkNoWitnessingTaskRestarts(testContext: TestContext) {
  testContext.info('Checking that no witnessing tasks crashed and restarted during the tests');

  const restartEvents = await prisma.event.findMany({
    where: { name: WITNESSING_TASK_RESTARTED_EVENT },
    include: { block: true },
    orderBy: { block: { height: 'asc' } },
  });

  if (restartEvents.length > 0) {
    const occurrences = restartEvents
      .map((event) => `block ${event.block.height}: ${JSON.stringify(event.args)}`)
      .join(', ');
    throw new Error(
      `A witnessing task crashed and was restarted during the tests, emitting ` +
        `${WITNESSING_TASK_RESTARTED_EVENT}. This indicates a panic or critical error in the ` +
        `engine that must be investigated. Occurrences: ${occurrences}`,
    );
  }
}
