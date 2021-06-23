# Rauk User Guide
__For rauk v0.0.0__

## Table of contents
1. [About](#about)
2. [Installation](#installation)
    1. [Requirements](#requirements)
    2. [Install binary release](#install-binary-release)
    3. [Building rauk](#building-rauk)
3. [Currenty supported crates](#currently-supported-crates)
4. [Using rauk](#using-rauk)

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
* Rust 1.51.0+

You need to make sure that you have the latest version of LLVM that is supported by KLEE installed!

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

