# RTIC Analysis Using KLEE 
> "A rauk is a column-like landform in Sweden and Norway, often equivalent to a stack."

**Rauk** is a program that automatically generates test vectors for your [RTIC](https://rtic.rs) application and uses them to
run a measurement-based WCET analysis on actual hardware.

## Table of contents
1. [Features](#features)
2. [Requirements](#requirements)
3. [Getting started](#getting-started)
4. [Limitations](#limitations)

## Features
- Automatic test vector generation of RTIC user tasks using KLEE
- Measurement-based WCET analysis of user tasks using the test vectors
- Response-time analysis of system from the WCET results

## Requirements
* [KLEE](https://github.com/klee/klee) v2.2+
* Linux x86-64

### Supported crates
* rtic v0.6+
* cortex-m v0.7+
* cortex-m-rt v0.6+

## Getting started

### Important
In order for Rauk to generate the test vectors you need to set a panic handler that aborts! Othewise it will not terminate. You can add the following
to your application:
```rust
#[cfg(feature = "klee-analysis")]
use panic_klee as _;
```
Rauk will patch that dependency by default, so there is no need to change anything inside your Cargo.toml!

### Test generation
Rauk can generate tests on the LLVM IR 

## Limitations
The following RTIC features are currently supported:
* Hardware tasks
* Resources
   * Primitives
* LateResources
    * Signed and unsigned integers
    * `char`
* Peripheral readings

## License
TBA
