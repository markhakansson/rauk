# Rauk User Guide
__For rauk v0.0.0__

## Table of contents
1. [About](#1-about)
2. [Installation](#2-installation)
    1. [Requirements](#21-requirements)
    2. [Install binary release](#22-install-binary-release)
    3. [Building rauk](#23-building-rauk)
3. [Currenty supported crates](#3-currently-supported-crates)
4. [Using rauk](#4-using-rauk)
    1. [Before running rauk](#41-before-running-rauk)
        1. [Aborting panic handler](#411-aborting-panic-handler)
        2. [Cargo profiles](#412-cargo-profiles)

## 1. About
Rauk is an analysis tool for [RTIC](https://rtic.rs) applications that can utilize [KLEE](https://github.com/klee/klee) to
analyze all executable paths for all user tasks using their accessed resources. It will generate test vectors for each tasks
accessed resources, which are resource values that led to those paths being explored.

The generated test vectors can then be used by rauk to run a measurement-based worst-case execution time (WCET) analysis on
actual hardware. 

The output of the measurment can then be used to verify the response time of all tasks and the overall program schedulability
(not included in rauk).

## 2. Installation
You can either download the latest release or compile it yourself. You need to make sure that the requirements
are met in either case.

### 2.1 Requirements
* [KLEE](https://github.com/klee/klee) v2.2+
* GNU/Linux x86-64 (host)
* An ARM Cortex-M microcontroller 
* Rust 1.51.0

You need to make sure that you have the latest version of LLVM that is supported by KLEE installed! Certain Linux
distributions can have newer versions of LLVM, which are not yet supported by KLEE.

### 2.2 Install binary release
TODO: No release done yet.

### 2.3 Building rauk
If you want to instead compile and build rauk yourself you can just simply clone this repository
and let Cargo build and install it for you.

```console
# Clone repository into current directory
$ git clone https://github.com/markhakansson/rauk.git

# Change working directory to repository
$ cd rauk

# Install rauk using cargo
$ cargo install --path .
```

## 3. Currently supported crates
Rauk does not work with every release or RTIC and its support crates. The currently supported crates
for this version of rauk are listed below. If you are using other versions not listed below, you might not
be able to run rauk properly.


| Crate         | Version      |
| :------------ | :----------- |
| cortex-m-rtic | __0.6.*__    |
| cortex-m      | __0.7.*__    |
| cortex-m-rt   | __0.6.*__    |

## 4. Using rauk

### 4.1 Before running rauk
Before running rauk on your RTIC application you will need to make some minor changes to your application.

#### 4.1.1 Aborting panic handler
In order for the test generation to terminate on a panic you will need an aborting panic handler. You can implement your own
panic handler that does this or you can use the panic handler crate provided for this use case.

You can set the `panic-rauk` crate as an optional dependency in your `Cargo.toml`. It will abort on panics.
```toml
[dependencies.panic-rauk]
git = "https://github.com/markhakansson/panic-rauk.git"
version = "0.1"
optional = true
```
Then you can mark your normal panic handler to not be used for the feature flag `klee-analysis` and the `panic-rauk` crate
to be used on it instead.
```rust
// original panic handler
#[cfg(not(feature = "klee-analysis"))]
use panic_halt as _;

// aborting terminal handler for rauk
#[cfg(feature = "klee-analysis")]
use panic_rauk as _;
```

#### 4.1.2 Cargo profiles
If you want to use all of rauks functionality you will also need to make some changes to the cargo profiles in your `Cargo.toml`. Specifically `lto = "thin"` needs to be set for both optimization profiles in order to run the WCET measurement
on hardware.
```toml
[profile.release]
codegen-units = 1
debug = true
lto = "thin"

[profile.dev]
codegen-units = 1 # better optimizations
debug = true # symbols are nice and they don't increase the size on Flash
lto = "thin" # better optimizations
```
