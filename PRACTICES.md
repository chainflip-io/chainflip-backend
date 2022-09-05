# Justification

The rust development team as a whole has identified problems in our development process. This document is intended to help us avoid these problems.

The core of the problems we identified are frequent disagreements, regarding both code and development operations, that do not conclude or result in agreement. This lead to frustration among the development team which caused a lack of dialog between developers, distrust among the team members, and developers wasting significant time disagreeing without making progressing on problems. Regularly issues or pull requests would be paralzed due to an extended heated discussion about a detail which did not result in agreement. Instead everyone ended up being entrenched and so discussion stopped as it felt no progress towards a conclusion or agreement could be made.

This document and the practices within serve to resolve these problems by:
- Making our practices highly visible
- Clearly signifying lingering disagreements should be resolved and conclusions recorded here
- Giving a strong foundation of common ground regarding development practices between all team members
- Providing a clear path for resolving disagreements

# Location

We choose to document our practices here inside this repository, so they are easily assessible and we can use Github's source control tools to manage the maintainence of this document.

# Requirements

Our practices must be:
- Clearly explained, as if they are ambiguous that will allow room for disagreement to persist.
- Well-reasoned otherwise we likely alienate some developers, as such each practice should be justified using logic, principles, ideals, and other practices.

# Maintainence

Simply create an issue on this repository regarding the modification to / problem with the practices, provide a justification for the change, and add the <span style="color:rgb(0,255,0)">practices</span> label. We intentionally chose not to not burden ourselves with a formal process to agree on modifications to the practices, but it is expected the whole team be given the opportinity to comment and a majority of the team should agree to a change. Once the modification is agreed on by the team, create a pull request to modify this document. That pull request should contain changes that ensure the codebase matches the new practices if applicable.

When documenting a practice you should write a clear explanation with a justification, and if at all possible some brief examples of its application. As without examples it is easy to miss-interpret the intention of a particular practice.

# Application

It is expected we "commit and disagree" with the contents of this document. If you feel a particular practice should be changed, you should still follow the practice until the team agrees to a change.

These practices should be used to help resolve disagreements. If that is not possible, the disagreement should be escalated so it can be resolved. If possible the conclusion should be recorded in this document either by removing, adding, or changing practices.

# Principles

These are the foundation of our practices...

## Prioritise Readability

Developers spend most of their time reading existing code rather that adding new code (citation needed?). We should therefore put extra effort into ensuring that the code is clear (not just the implementation, but the code's intent too).

Specifically, readability should be prioritised over code's performance where the performance does not matter (e.g. don't optimise prematurely, particularly if the optimisation makes the code more complex). Clever tricks should in general be avoided if it leads to code that takes a considerable amount of effort to parse (and there is a reasonable, if less perfect or clever, alternative).

## Good over Perfect (aka 80/20 rule)

As developers we tend to be perfectionists spending way more time than necessary trying to get it perfect, following the best practices etc. While it is important to maintain high standards (especially if there are big security or performance implications), we should always ask ourselves whether the potential benefits are worth the extra time and the increased complexity.

Most importantly, code tends to get outdated as the system requirements change and our understanding of what we are building improves. Such code needs to be modified, rewritten or even deleted altogether, and the extra effort we spend trying to get it perfect is wasted.

# Ideals

# Practices

## Code

## Development Operations

### Submitting Code Changes

### Issues

### Release Process
