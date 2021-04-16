# RTIC Analysis Using KLEE 
> "A rauk is a column-like landform in Sweden and Norway, often equivalent to a stack."

**Rauk** is a program that automatically generates test vectors for your [RTIC](https://rtic.rs) application and uses them to
run a measurement-based WCET analysis on actual hardware.

## Table of contents
- [Features](#features)
- [Requirements](#requirements)
- [Getting started](#getting-started)
- [Limitations](#limitations)

## Features
- Test vector generation of RTIC user tasks using KLEE
- WCET analysis of user tasks
- Response-time analysis of system

## Requirements
* [KLEE](https://github.com/klee/klee) v2.2+
* RTIC v0.6+
* Linux

## Getting started

### Important
In order for Rauk to generate the test vectors you need to set a panic handler that aborts! Othewise it will not terminate. You can add the following
to your application:
```rust
#[cfg(feature = "klee-analysis")]
use panic_klee as _;
```
Rauk will patch that dependency by default, so there is no need to change anything inside your Cargo.toml!

## Limitations
The following RTIC features are currently supported:
* Hardware tasks
* Resources
    * Integer types
* LateResources
    * Integer types
