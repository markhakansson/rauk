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
        3. [Marking tasks for analysis](#413-marking-tasks-for-analysis)
5. [Advanced usage](#5-advanced-usage)
6. [Settings](#6-settings)

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
Rauk is supposed to be run in the following order:

1. `generate` - to generate test vectors
2. `flash` - to flash the binary used for WCET measuring to hardware
3. `measure` - to measure the WCET on the binary using the test vectors

All rauk commands and flags can be exposed by running rauk with the help flag.
```console
rauk --help
```

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
If you want to use all of rauks functionality you will also need to make some changes to the cargo profiles in your `Cargo.toml`. Specifically `lto = "thin"` and `debug = true` needs to be set for both optimization profiles in order
to run the WCET measurement on hardware.
```toml
[profile.release]
codegen-units = 1
debug = true
lto = "thin"

[profile.dev]
codegen-units = 1
debug = true
lto = "thin"
```

#### 4.1.3 Marking tasks for analysis
You can mark the tasks you want rauk to analyse with the `#[rauk]` attribute. Rauk will ignore all other tasks that
are not marked.

```rust
#[rauk] // <-- mark task for analysis
#[task(...)]
fn task(_: task::Context) {
    // code
}
```
_NOTE_: You will need to remove or comment out these attributes when running or building your application for your regular targets. They are part of a custom RTIC syntax used by rauk. As for now there are no convenient workaround for this.

### 4.2 Generating tests
Test vectors can be generated by the `generate` command. 

```console
rauk-generate 0.0.0
Generate test vectors using KLEE

USAGE:
    rauk generate [OPTIONS] [FLAGS] 

FLAGS:
        --emit-all-errors    Emit all KLEE errors
    -h, --help               Prints help information
    -r, --release            Build artifacts in release mode
    -V, --version            Prints version information

OPTIONS:
    -b, --bin <bin>            Name of the bin target
    -e, --example <example>    Name of the example target
```
For example, to generate test vectors for a binary target with the name `hello` in release mode:
```rust
rauk generate --bin hello --release
```
The output can be easily accessed via a symlink in `target/rauk/klee-last/`. You can display the contents of each test
vector using `ktest-tool`.

_NOTE_: If building tests in release mode, make sure to set the flag for `flash` and `measure` commands. Otherwise you might have problems!

### 4.3 Flashing to hardware
The binary used for WCET measurement can be built and flashed to hardware using the `flash` command.

```console
rauk-flash 0.0.0
Build and flash the replay harness onto the target hardware

USAGE:
    rauk flash [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -r, --release    Build artifacts in release mode
    -V, --version    Prints version information

OPTIONS:
    -b, --bin <bin>            Name of the bin target
    -c, --chip <chip>          The name of the chip to flash to
    -e, --example <example>    Name of the example target
    -t, --target <target>      The target architecture to build the executable for
```
The supported chip correspond to `probe-rs` targets which can be viewed at [target-gen](https://github.com/probe-rs/target-gen).

For example to flash the binary on an `STM32F401RETx` chip using the `thumbv7em-none-eabi` toolchain we do:
```rust
rauk flash --bin hello --release --chip STM32F401RETx --target thumbv7em-none-eabi
```
### 4.4 Measure
To measure a flashed binary built for WCET measurment can be done with the `measure` command.

```console
rauk-measure 0.0.0
WCET measure for each task using the test vectors on the replay harness

USAGE:
    rauk measure [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -r, --release    Build artifacts in release mode
    -V, --version    Prints version information

OPTIONS:
    -b, --bin <bin>            Name of the bin target
    -c, --chip <chip>          The name of the chip to flash to
    -d, --dwarf <dwarf>        Path to DWARF
    -e, --example <example>    Name of the example target
    -k, --ktests <ktests>      Path to KLEE tests
```
For example to measure the previous binary we do:
```rust 
rauk measure --bin hello --release --chip STM32F401RETx 
```
The complete output will be stored at `target/rauk/rauk.json`.

## 5. Advanced usage

## 6. Settings
