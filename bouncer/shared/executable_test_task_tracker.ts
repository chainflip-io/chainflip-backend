import { ExecutableTest } from './executable_test';

export const runningTests = new Set<ExecutableTest>();

// Look at the call stack and find a known test file, matching against the list of all running test. Returns the test and the location of the call.
function getTestFromStack(): { test: ExecutableTest; location: string } | undefined {
  try {
    const fakeError = new Error();

    if (fakeError.stack !== undefined) {
      const stack = fakeError.stack.split('\n');
      for (let i = 1; i < stack.length; i++) {
        const [fileName, lineNumber] = stack[i].split(':');
        for (const test of runningTests) {
          if (fileName?.includes(test.fileName)) {
            return { test, location: `${test.filePath}:${lineNumber}` };
          }
        }
      }
    }
  } catch (e) {
    console.error(e);
  }

  return undefined;
}

export class TaskTracker {
  private test: ExecutableTest | undefined;

  private taskId: number | undefined;

  constructor(name: string) {
    const testDetails = getTestFromStack();
    if (testDetails !== undefined) {
      this.test = testDetails.test;
      this.taskId = testDetails.test.startAwaiting(name, testDetails.location);
    }
  }

  stopAwaiting() {
    this.test?.stopAwaiting(this.taskId);
  }
}

// Use to detect where timeouts are happening in tests.
// Call at the start of any utility function that is expected to take some time to compete.
// It returns a TaskTracker object that can be use to stop the tracking when the task is completed.
// Please remember to stop the tracker at all places the function stops.
export function startAwaitTask(name: string): TaskTracker {
  return new TaskTracker(name);
}
