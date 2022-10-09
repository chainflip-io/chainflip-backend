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

# Ideals

# Practices

## Prioritise Readability

Developers spend most of their time reading existing code rather that adding new code. We should therefore put extra effort into ensuring that the code is clear (not just the implementation, but the code's intent too).

Specifically, readability should be prioritised over code's performance where the performance does not matter (e.g. don't optimise prematurely, particularly if the optimisation makes the code more complex). Clever tricks should in general be avoided if it leads to code that takes a considerable amount of effort to parse (and there is a reasonable, if less perfect or clever, alternative).

## Good over Perfect (aka 80/20 rule)

As developers we tend to be perfectionists spending way more time than necessary trying to get it perfect, following the best practices etc. While it is important to maintain high standards (especially if there are big security or performance implications), we should always ask ourselves whether the potential benefits are worth the extra time and the increased complexity.

Most importantly, code tends to get outdated as the system requirements change and our understanding of what we are building improves. Such code needs to be modified, rewritten or even deleted altogether, and the extra effort we spend trying to get it perfect is wasted.

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