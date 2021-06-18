# rauk - RTIC Analysis Using KLEE 
<p align="center">
 <img src=".github/drawing.png" width="300" height="300">
</p>

> "A rauk is a column-like landform in Sweden and Norway, often equivalent to a stack."

**Rauk** is a program that automatically generates test vectors for your [RTIC](https://rtic.rs) application and uses them to
run a measurement-based WCET analysis on actual hardware. 

## Warning!
__Please note that Rauk is still very early in development and should not be used for serious or production-ready applications!__

## Table of contents
1. [Features](#features)
2. [Requirements](#requirements)
3. [Getting started](#getting-started)
4. [How it works](#how-it-works)
5. [Limitations](#limitations)
6. [License](#license)

## Features
- Automatic test vector generation of RTIC user tasks using the symbolic execution engine [KLEE](https://github.com/klee/klee)
- Measurement-based WCET analysis of user tasks using the test vectors an real hardware
- The output can be used to calculate the response-time of all tasks

## Requirements
* [KLEE](https://github.com/klee/klee) v2.2+
* GNU/Linux x86-64 (host)
* An ARM Cortex-M microcontroller 

### Supported crates & versions

| Crate         | Version  |
| :------------ | :------- |
| cortex-m-rtic | 0.6.*    |
| cortex-m      | 0.7.*    |
| cortex-m-rt   | 0.6.*    |


## Getting started

### Important!
In order for Rauk and KLEE to generate the test vectors you need to set a panic handler that aborts! Othewise it will not terminate. You can add the following
to your application:
```rust
#[cfg(feature = "klee-analysis")]
use panic_rauk as _;
```
You will have to add that as a dependency and also enable LTO and debug information in your `Cargo.toml`
```toml
# Cargo.toml
[dependencies.panic-rauk]
git = "https://github.com/markhakansson/panic-rauk.git"
optional = true

[profile.dev]
codegen-units = 1
lto = "thin"
debug = true

[profile.release]
codegen-units = 1
lto = "thin"
debug = true
```
### Quickstart
Running Rauk for a binary target without release optimizations: 
1. Build test harness and generate test vectors
    - `rauk generate --bin <NAME>` or 
2. Build replay harness and flash it to hardware
    - `rauk flash --bin <NAME> --target <TARGET> --chip <CHIP>`
3. Measure replay harness to get WCET trace
    - `rauk measure --bin <NAME> --chip <CHIP>`

## How it works
The basics of Rauk is actually pretty simple. It first creates a test harness based on the RTIC application to be tested, 
where it marks task resources and hardware readings for KLEE to work on symbolically. KLEE will generate test vectors for 
each user task this way. The test vectors created for each task will result in all paths of the task being reached. Using
these vectors it is assumed that one of these vectors will result in the longest path of the task being run. 

Then Rauk creates a replay harness where all entry and exitpoints of task handlers and resource locks (critical sections)
are inserted with a breakpoint. Then it will write the contents of each test vector and at each breakpoint it stops at,
measure the cycle count. This will result in a trace for each test vector, which can be used to run a response-time analysis
given further information.

See [RAUK: Embedded Schedulability Analysis Using Symbolic Execution](https://github.com/markhakansson/master-thesis) (incomplete)
for the thesis that resulted in this application.

## Limitations
* The way WCET measuring is done, does add some overhead to the results
* KLEE generates test vectors on LLVM IR which does not necessarily mean that the test vectors will target the longest path in ARM instructions
* No monotonics support (yet)
* Only simple resources are supported (such as primitives, peripherals and pins)
* Works best on applications that have smaller tasks that generally have short a runtime
* `ìnit()` and `idle()` functions are ignored completely

## Acknowledgements
Rauk was heavily inspired by the works of Lindner et al. [1] were they used KLEE to run a hardware-in-the-loop WCET analysis of RTFM (the old name of RTIC). And would not have been possible without their contributions.

## References
[1] Lindner, M., Aparicio, J., Tjäder, H., Lindgren, P., & Eriksson, J. (2018). Hardware-in-the-loop based WCET analysis with KLEE. 2018 IEEE 23RD INTERNATIONAL CONFERENCE ON EMERGING TECHNOLOGIES AND FACTORY AUTOMATION (ETFA), 345–352.

## License
TBA
