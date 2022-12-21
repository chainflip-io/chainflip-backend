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

## Reviewers First

Code reviewing is an important part of the software development lifecycle. It is the barrier between developer and production. In order to ensure reviews are most efficient and effective, we believe it's best to think about creating a PR from the perspective of the person who will be reviewing it. The following guidelines aim to assist in improving the quality of the review process.

### Small PRs

Why are small PRs important? [Google has a good explanation](https://google.github.io/eng-practices/review/developer/small-cls.html) and we agree.

A rough guideline for the maximum of what constitutes "small" is around 400 lines of code. [Research](https://static1.smartbear.co/support/media/resources/cc/book/code-review-cisco-case-study.pdf) shows that once developers had reviewed more than 200 lines of code their ability to identify defects diminished. So ideally PRs kept under 200 lines are best. It's also important to make sure the PR makes sense on its own if merged. Develoeprs should think about how a large change can be broken up into smaller, self-contanied changes where relevant, to ensure PRs stay small.

### Organised and meaningful commits

Each commit should be well named. While writing the occasional "ARGHHH" commit meessage may give us the stress relief we need, we should instead seek other stress reduction strategies, and have well named commits like "fix: authority rewards are now rewarded at each block" so as not to annoy and stress your fellow developers (which might result in "ARGGGH"s with more "G"s). This is important for two reasons: debugging, to find where bugs were introduced and providing reviewers a nice description of what to expect in a commit, also allowing them to navigate your PR more easily.

### Self review

Before requesting a review, developers are encouraged to conduct a thorough self-review of their own work. The approach to self-reviewing should be the same approach taken to reviewing the work of others. Investigate assumptions are being made, look for bugs, try and break it, think about how it could be structured to read better. Often you'll be able to find ways to improve your own code before the reviewer does.

## Correctness

The Chainflip network will be securing significant amounts of funds for its users. The protocol has been designed with security and token-economic incentives in mind, but none of this matters if the code does not implement the design correctly. Therefore, before merging any code to `main` we should be able to reason about its correctness and demonstrate that our code meets requirements.

Correctness in this instance means that we know what the code should do; that it indeed fulfils its stated purpose; that we have considered all edge cases. In the spirit of readability, correctness should be as easy as possible to reason about by anyone reading, reviewing or auditing the code.

The strongest guarantee of correctness is enforced through the type system. Supporting this, we can use unit testing, property-based testing, or integration testing. Where unenforced assumptions are made, these should be clearly documented using (doc-)comments.

## Maintainability

According to Google:

> Code maintainability is a capability that requires organization-wide coordination, since it relies on being able to search, reuse, and change other teams' code.

Maintainability is one of the main drivers of a team's success and ability to move forward. Wherever possible we should actively resist accumulating technical debt, and reduce it when we have the opportunity. As long as we follow our other practices, we should be working towards this ideal.

# Practices

A set of practices we follow within the backend repo at Chainflip.

## Prioritise Readability

Developers spend most of their time reading existing code rather that adding new code. We should therefore put extra effort into ensuring that the code is clear (not just the implementation, but the code's intent too).

Specifically, readability should be prioritised over code's performance where the performance does not matter (e.g. don't optimise prematurely, particularly if the optimisation makes the code more complex). Clever tricks should in general be avoided if it leads to code that takes a considerable amount of effort to parse (and there is a reasonable, if less perfect or clever, alternative).

## Good over Perfect (aka 80/20 rule)

As developers we tend to be perfectionists spending way more time than necessary trying to get it perfect, following the best practices etc. While it is important to maintain high standards (especially if there are big security or performance implications), we should always ask ourselves whether the potential benefits are worth the extra time and the increased complexity.

Most importantly, code tends to get outdated as the system requirements change and our understanding of what we are building improves. Such code needs to be modified, rewritten or even deleted altogether, and the extra effort we spend trying to get it perfect is wasted.

## Commit names

We use a variation on conventional commits to keep our commits clear for reviewers and to encourage meaningful commits.

`feature/feat`: Functionality has been extended in some way.
`fix`: The code has been changed to fix a bug.
`refactor`: The code has been changed but the changes do not affect the behaviour.
`test`: Test code has been added or changed (though tests will often/usually be included as part of a `feat` or `fix` commit).
`chore`: Code has been moved, dead code removed, file renamed, merging imports, function name typos, changing module structure, bumping dependency versions etc. There shouldn't be any code changes in a `chore` commit.
`doc`: Comments, READMEs and any other form of documentation e.g. diagrams have been changed.

The order of prececedece is as listed. So if a commit includes a feature and test changes you'd use `feature`.

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
