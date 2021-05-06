# RTIC Analysis Using KLEE 
> "A rauk is a column-like landform in Sweden and Norway, often equivalent to a stack."

**Rauk** is a program that automatically generates test vectors for your [RTIC](https://rtic.rs) application and uses them to
run a measurement-based WCET analysis on actual hardware.

## Table of contents
1. [Features](#features)
2. [Requirements](#requirements)
3. [Getting started](#getting-started)
4. [How it works](#how-it-works)
5. [Limitations](#limitations)
6. [License](#license)

## Features
- Automatic test vector generation of RTIC user tasks using KLEE
- Measurement-based WCET analysis of user tasks using the test vectors
- Response-time analysis of system from the WCET results

## Requirements
* [KLEE](https://github.com/klee/klee) v2.2+
* Linux x86-64 (host)

### Supported crates
* cortex-m-rtic v0.6+
* cortex-m v0.7+
* cortex-m-rt v0.6+

## Getting started

### Important!
In order for Rauk to generate the test vectors you need to set a panic handler that aborts! Othewise it will not terminate. You can add the following
to your application:
```rust
#[cfg(feature = "klee-analysis")]
use panic_klee as _;
```
Rauk will patch that dependency by default, so there is no need to change anything inside your Cargo.toml!

### Quickstart

1. Build test harness and generate test vectors
    - `rauk generate --bin <NAME>` or `rauk generate --example <NAME>`
2. Build replay harness and flash it to hardware
    - `rauk flash --target <TARGET> --chip <CHIP>`
3. Measure replay harness to get WCET trace
    - `rauk analyze --chip <CHIP>`

## How it works
The basics of Rauk is actually pretty simple. It first creates a test harness based on the RTIC application to be tested, where it marks task resources and 
hardware readings for KLEE to work on symbolically. KLEE will generate test vectors for each user task this way. The test vectors created for each task will result in all paths of the task being reached. Using these vectors it is assumed that one of these vectors will result in the longest path of the task being run. 

Then Rauk creates a replay harness where all entry and exitpoints of task handlers and resource locks (critical sections) are inserted with a breakpoint. 
Then it will write the contents of each test vector and at each breakpoint it stops at, measure the cycle count. This will result in a trace for each test vector, which can be used to run a response-time analysis given further information.

See [RAUK: Embedded Schedulability Analysis Using Symbolic Execution](https://github.com/markhakansson/master-thesis) (incomplete) for the thesis that resulted in this application.

## Limitations
Rauk does have a few limitations, notably that the measurement-based analysis is not perfect. The way the measuring is done does add some overhead.


The following RTIC features are currently supported:
* Hardware tasks
* Resources
   * Primitives
* LateResources
    * Signed and unsigned integers
    * `char`
* Peripheral readings

* 

## License
TBA
