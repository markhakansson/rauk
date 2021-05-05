# Information about all forks/patches used by rauk
This documents what forks rauk is using to patch the RTIC application's `Cargo.toml` file. 
Also why they are patched and what has been changed in each fork compared to the original.

The forks and patches should be kept as simple as possible and should be compatible with an RTIC application without
the features added enabled.

# cortex-m-rtic
The `cortex-m-rtic`crate has been forked and patched in order to run the rauk test generation as well as the WCET measurement on actual hardware.
It has been extended with the features `klee-analysis` and `klee-replay` in order to do so.

## Feature: `klee-analysis`
This feature changes the macro expansion in RTIC to create a test harness for generating test vectors with KLEE. It also sets the resources symbolic of the tasks
that are to be tested.

### Test harness
The test harness is just a match statement on an integer that is made symbolic. Each arm contains the resources of a single that are set to be symbolic, 
and finally a function call to the task itself. An example can be seen below.

```rust
/// KLEE test harness
mod rtic_ext {
   use super::*;
   use klee_sys::klee_make_symbolic;
   #[no_mangle]
   // Notice that the main does not return ! as it will terminate
   unsafe extern "C" fn main() {
     let mut task_id = 0;
     klee_make_symbolic!(&mut task_id, "__klee_task_id");
     match task_id {
       0 => {
         // Set the tasks resource to be symbolic
         klee_make_symbolic!(&mut __rtic_internal_resource_one, "__rtic_internal_resource_one);
         // Then call the task
         crate::app::task_one(task_one::Context::new(&rtic::export::Priority::new(1)));
       },
       _ => ()
     }
     // main will terminate and KLEE will return its result
   }
} 
```

### Changes to `cortex-m-rtic-macros`
* `codegen.rs`
  * Changes the `main` function of the expanded macro to only include the KLEE test harness 

### Additions to `cortex-m-rtic-macros`
* `codegen/klee.rs`
  * Generates the KLEE test harness with task execution and symbolic resources
  * Sets all user tasks resources to be symbolic


## Feature: `klee-replay`
This feature changes the macro expansion's main function to only contain the rauk replay harness. 

### Replay harness
The replay harness contains a match statement of an integer
where each arm is a function call to a single task handler. The matching integer as well as the task's resources are set via the debugger from the contents of the 
generated test vectors.

Inside of each task handler and outside each resource lock (critical section) there are an entry and exit breakpoint. They are numbered depending on which type of entry and exit it is. They are used to read the current cycle counter in order to measure the WCET using the given test. There are also a breakpoint inside a task
and a resource lock, used to retrieve the name of the task/resource that is executed/accessed.

An example of the replay harness can be seen below.
```rust
// Global so to not be out optimized
static mut __klee_task_id: u8 = 0;

/// KLEE replay harness
mod rtic_ext {
   use super::*;
   #[no_mangle]  
   unsafe extern "C" fn main() -> ! {
      // assertion statements and pre init statements
      //...

      // Enable trace
      core.DCB.enable_trace();
      core.DWT.enable_cycle_counter();

      // Loop until test cases are finished
      loop {
         // Reset CYCCNT after each loop 
         core.DWT.cyccnt.write(0);
         /// Stop before matching tasks in order to set the resources 
         asm::bkpt_imm(255);
            match __klee_task_id {
               0 => {
                  // "task_one" task handler on EXTI0
                  EXTI0();
               }
               _ => ()
            }
         }
      }
}
```

### Changes to `cortex-m-rtic`
* `export.rs`
  * Set a breakpoint inside a resource lock to denote `InsideLock`in order to retrieve the name of the resource

### Changes to `cortex-m-rtic-macros`
* `codegen.rs`
  * Changes the `main` function of the expanded macro to only include the replay harness
* `codegen/utils.rs`
  * Set entry and exit breakpoints inside the mutex implementation of a resource
* `codegen/hardware_task.rs`
  * Set entry and exit breakpoints inside the task handler
  * Set a breakpoint to denote `InsideTask` inside of the task 
* `codegen/dispatchers.rs`
  * Set entry and exit breakpoints inside the task dispatcher

### Additions to `cortex-m-rtic-macros`
* `codegen/klee_replay.rs`
  * Generate the replay harness used for measuring WCET on hardware
  * Enables trace and cycle counter
  * WIP: software task support

# cortex-m
The `cortex-m` crate has been patched to make register readings and certain hardware accesses return symbolic values for `klee-analysis` mode.
And the breakpoint function can take immediate values as an argument which is used in `klee-replay` for WCET analysis.

## Additions to `cortex-m`
* `asm/inline.rs`
  * Breakpoint function takes a value for its immediate value
* `asm/lib.rs`
  * Breakpoint function takes an immediate value
* `src/asm.rs`
  * Add an extra breakpoint function with an immediate value as parameter

## Feature: `klee-analysis`
### Changes to `cortex-m`
* Most (if not all) function calls that return values from any hardware accesses (readings etc.) return symbolic values

# vcell
`vcell` is not usually a part of a user's `Cargo.toml` file but is used by underlying crates. This is patched such that when creating the test harness and
running KLEE, hardware accesses can be tested via the added `klee-analysis` feature. Volatile writes will be ignored when generating tests as there are no side effects of that operation. But volatile reads will be made symbolic as it's when reading and dealing with the result as errors can be uncovered.

The feature `klee-replay` is used during the anaysis phase and sets a breakpoint inside the vcell reading. This is used to overwrite the read value with the contents of a test vector.

## Feature: `klee-analysis`
### Changes to `vcell`
* `lib.rs`
  * `set`does nothing
  * `get`and `as_ptr` will return a symbolic value

## Feature: `klee-replay`
### Changes to `vcell`
* `lib.rs`
  * `get` and `as_ptr` contains a breakpoint to denote `InsideLockClosure`

# cortex-m-rt
The cortex-m-rt crate has been patched with a feature that currently does nothing. This in order to not fetch multiple versions of the crate. If multiple versions
of this crate are used, KLEE can not test the LLVM IR as there will be undefined and unlinked references to external globals.

## Feature: `klee-analysis`
No changes.
