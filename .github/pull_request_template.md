# Pull Request

Closes: PRO-xxxx

## Summary

*Please include a succinct description of the purpose and content of the PR. What problem does it solve, and how? Link issues, discussions, other PRs, and anything else that will help the reviewer.*

---

## *Checklist [feel free to add/remove/edit as appropriate but please don't ignore]*

Before marking this as ready for review, consider the following:

* The PR is complete:
  * [ ] Scope is clear and fully implemented.
  * Tests:
    * [ ] Unit tests for isolated local conditions.
    * [ ] Proptests for important invariants.
    * [ ] Integration tests for runtime-wide behaviours.
    * [ ] Bouncer tests for E2E features involving external chains.
  * [ ] Benchmarks: did you leave any dangling placeholders?
  * [ ] Migrations: if you wrote one, did you test it using try-runtime?
  * [ ] Bouncer typegen: if you changed any runtime types you might need to update this.
* Make life easy for the reviewer:
  * [ ] Intended behaviours are clear and, where possible, covered by tests.
  * [ ] Unrelated changes are isolated in dedicated commits.
  * [ ] Interfaces are clearly documented.
  * [ ] Anything non-obvious is documented in the `Summary` section above.
* [ ] Consider asking an AI for a local review to catch any embarrassing mistakes early.
* [ ] Vice-versa: read your code carefully to ensure no AIs added anything embarrassing.
