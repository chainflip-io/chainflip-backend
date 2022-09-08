# Justification

The intention of this document is to ensure the team makes decisions, work can move forward, and the team members are comfortable with those decisions, even when the members initially do not agree.

# Location

We choose to document our practices here inside this repository, so they are easily assessible and we can use Github's source control tools to manage the maintainence of this document.

# Requirements

Our practices must be:

- As unambiguous as possible. It should be clear how and when the practice applies. If necessary they should include clarifying examples. Ideally these examples would be from our codebase, and include links to PRs and issues.
- Well-reasoned. Otherwise we will likely alienate some developers. As such each practice should be justified using logic, principles, ideals, and other practices.

# Maintainence

Create an issue on this repository regarding the modification to / problem with the practices, provide a justification for the change, and add the <span style="color:rgb(0,255,0)">practices</span> label. We intentionally chose not to burden ourselves with a formal process to agree on modifications to the practices, but it is expected the whole team be given the opportinity to comment, a majority of the team should agree to a change, and those who disagreed are satisfied with the resolution. Once the modification is agreed on by the team, create a pull request to modify this document. That pull request should contain changes that ensure the codebase matches the new practices if applicable.

# Application

It is expected we "disagree and commit" with the contents of this document. If you feel a particular practice should be changed, you should still follow the practice until the team agrees to a change.

These practices should be used to help resolve disagreements. If that is not possible, the disagreement should be escalated so it can be resolved. If possible the resolution should result in modifcations to this document.

# Template

# Principles

These are the foundation of our practices...

## Correctness

The Chainflip network will be securing significant amounts of funds for its users. The protocol has been designed with security and token-economic incenties in mind, but none of this matters if the code does not implement the design correctly. Therefore, before merging any code to `develop` we should be able convince ourselves that it is correct.

Correctness in this instance means that we know what the code should do; that it indeed fulfils its stated purpose; that we have considered all edge cases. In the spirit of readability, correctness should be as easy as possible to reason about by anyone reading, reviewing or auditing the code.

The strongest guarantee of correctness is enforced through the type system. Supporting this, we can use unit testing, property-based testing, or integration testing. Where unenforced assumptions are made, these should be clearly documented using (doc-)comments.

## Maintainability

According to Google:

> Code maintainability is a capability that requires organization-wide coordination, since it relies on being able to search, reuse, and change other teams' code.

Maintainability is one of the main drivers of a team's success and ability to move forward. Wherever possible we should actively resist accumulating technical debt, and reduce it when we have the opportunity. As long as we follow our other practices, we should be working towards this ideal.

# Ideals

# Practices

## Code

## Development Operations

### Submitting Code Changes

### Issues

### Release Process

# Code Reviews

The principles above are primarily to be applied by those submitting PRs. However, there are some principles that apply more specifically to when one is conducting a review of someone else's work.

Improving the speed and effectiveness of code reviews directly improves the speed and effectiveness of the team.

## Better not perfect

If a PR improves the overall quality of the code base, reviewers should tend towards approving over holding up the PR. Assuming there are no bugs introduced, this may mean approving with some comments, perhaps suggesting that some improvements be taken out into their own issue, or for even smaller items, allowing the developer to make a judgement call on whether to include it before merging.
