# FAQ

## When executing `rauk` the build fails, what to do?
`rauk` needs to patch your project's `Cargo.toml` with its own forks and dependencies in order to to generate tests and run the analysis.
This can result in conflicts with your cached dependencies. You can solve this by
* First clearing the cache with `cargo clean`
* If that does not work, try deleting the `Cargo.lock` file

## Has Rauk been tested by KLEE?
Ironically, no.
