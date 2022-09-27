# Feature flags

Chainflip uses feature flags to gate new functionality while incomplete, or unready for launch for some other reason.

## Why feature flags?

One way to explore why feature flags are useful is to do a comparison with the alternative, long-living feature branches.

There are several downsides to the long-living feature branch approach:
- When the feature is ready to merge to the main branch, merge conflicts may result. The longer the branch is around, the more likely it is conflicts exist. This isn't just annoying for the developer that has to do it, but takes an unnecessary amount of time. It's also a potential source of introducing bugs, if the conflicts are not resolved correctly. If we have a long running feature branch, where other branches are merged into that, with that branch receiving merges or being rebased onto its base, this quickly becomes very complex.
- Often when features are added, refactors are made to facilitate the adding of that feature. If someone else wishes to work on something tangential to the refactor, do they implement the refactor as well on their branch, which is off main? Or do they build off the long-living feature branch? These scenarios can then become entangled with each other, worsening the issue.
    There are consequent issues that arise by continuing this way:

    a) the developer is forced to deal with the conflicts that arise (taking time) - after they've already spent the time to work out exactly how their work fits in to both.

    b) forced to *delay* the work, marking it as "blocked" - loosing team productivity. 


Perhaps the best way to describe the benefits of feature flags is that they ensure developers are always working on as close as possible to the latest code. Combined with small PRs, and fast review cycles, feature flags allow for extra fast development cycles, as engineers are less likely to duplicate work and are rarely blocked due to another developer's work. Nor do they have to spend time thinking about how to branch, rebase, or where to put their work to avoid conflicts, or whether to do the work now at all.

Feature flags do create some noise in the codebase. This is unavoidable, but there are ways this can be reduced. Often you can structure the code so that the feature flagged functionality isn't interleaved with other code (where you might have to write a couple of `#[cfg(feature = "my_feature")]`s, and instead just write a single one outside a block, as well as including imports into these blocks.

## Usage

We use rust/cargo's built in [features](https://doc.rust-lang.org/cargo/reference/features.html) to allow us to do compile-time feature flagging.
Here are some feature flag use guidelines to ensure we use them effectively, and avoid some of the pitfalls.

### Use guidelines
- Try not to use them at all. They should be used sparingly, and only when necessary. 
- They should be as short-lived as possible. Maintain and clean them aggressively, once a feature flagged feature has been released, all occurrences of that flag should be deleted from the codebase. 
- *Never* repurpose feature flags. Create a new one for something newm, or [risk losing half a billion dollars](https://blog.statsig.com/how-to-lose-half-a-billion-dollars-with-bad-feature-flags-ccebb26adeec)
- The codebase must pass CI i.e. no warnings, tests passing *no matter which features are enabled*
- Tests that are for feature flagged functionality can just be in their own module, with the feature flag added to the module. Test behaviour should not change based on a feature flag. Any test depending on a flag should be either enabled or disabled completely by that flag.