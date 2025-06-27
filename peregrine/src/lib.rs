//! # Peregrine Engine
//!
//! A discrete event spacecraft simulation engine designed for schedulers.
//!
//! Peregrine always does the minimal amount of computation to respond to changes in the plan, and to
//! calculate only the requested resources *at the requested times*. If you only care about a couple
//! resources in the vicinity of a small plan change, then that's all the engine simulates.
//!
//! Peregrine also stores a long-term history of resource states, meaning that simulation is not just
//! incremental with respect to the most recent plan state; it is incremental with respect to all recorded
//! history. If you undo five recent simulated changes and add one new activity, the engine will only
//! simulate change of adding the activity, not of adding one and deleting five. The engine can reuse
//! any state that it has encountered before, whether it comes from the current simulation, past simulations,
//! other plans, other models that share resources, or any combination of those.
//!
//! Peregrine performs all simulation with as much parallelism as is mathematically allowed by the
//! configuration of the plan. Even on linear plan structures with no available concurrency, initial (extremely informal) benchmarking
//! suggests that Peregrine's engine overhead is significantly lower than Aerie-Merlin's - simulating
//! millions of simple operations per second instead of thousands. Highly expensive operations may
//! amortize the overhead differences, but will not amortize the parallelism.
//!
//! Peregrine also has generalized "dynamic" resources, meaning resources that can vary even when
//! not being actively operated on. This is akin to Merlin's linear resources, but abstracted to any
//! function of any data type. In addition to the nicer ergonomics and better performance, this gives
//! the modeler the extremely powerful ability to express the hypothetical future evolution of a resource,
//! if it is left untouched. As far as I know, this is the only selling point of Peregrine that
//! could also be implemented in Merlin.
//!
//! ## Concepts
//!
//! The engine simulates the evolution of a set of Resources over time, operated on by a
//! set of instantaneous Operations, which themselves are grouped together into Activities.
//!
//! ### Resources & Activities
//!
//! Resources are just variables whose evolution over time is tracked and recorded. Activities
//! contain operations that mutate those resources, and place those operations at mostly-predetermined
//! times throughout a plan. This is the only fundamental difference between Peregrine and
//! Merlin; activities declare their operations - when they happen, what resources they read/write -
//! and their total duration ahead of time, before simulation. There is support for operations with
//! flexible timing, but it will hurt performance if used extensively.
//!
//! ### Operations, Dependencies, & Parallelism
//!
//! Operations are the instantaneous discrete events that the engine simulates. The can read and write
//! resources, access activity arguments, and be configured by the activity (ahead of time only).
//! By forcing you to declare which resources you read and write, the engine is able to build a
//! directed acyclic graph of operation dependencies. This DAG enables most of the parallelism and minimal
//! computation I bragged about in the intro. When you make a change and request a view of a resource,
//! the simulation propagates backward through the DAG from the requested range, and evaluation
//! of branches in the graph immediately stop when they find cached values.
//!
//! ### History & Incremental Simulation
//!
//! Peregrine records the history of all operations that have been simulated in the current session. This enables
//! the engine to immediately stop as soon as it encounters a state that it has been in before. This is
//! done by hashing the input resources that each operation reads, together with the operation's unique ID.
//! So if the simulation has some resources that often have cyclical states, the engine will reuse the
//! history not just from previous runs, but from the current run as well.
//! If the resource reads are not hashable, it defaults to a structural hash which will likely have fewer
//! cache hits.
//!
//! Importantly, Peregrine stores history independent of the plan, meaning that it can be shared between
//! branched versions of the same plan, even as they are updated and simulated live, in parallel.
//! For an extremely simplified example, consider a plan working on two mostly-independent subsystems,
//! `A` and `B`. We start with an unsimulated base plan, then branch into two copies for the `A` and
//! `B` teams to work on. Say team `A` simulates their portion of the base plan first. `B`'s work is
//! only *mostly* independent, with some coupling through common resources. Most of the time, `B` doesn't
//! need `A`'s resources, but if they do, `A` has already simulated the base plan and those results can
//! be reused even though they are on a different branch. Then, when the branches are merged, a majority
//! of the final plan has already been simulated. Only the areas that coupled `A` and `B` together need
//! to be resimulated.
//!
//! This approach's main drawback is memory usage. By indiscriminately storing all sim results without
//! knowing if they will ever be reused, it can build up gigabytes of store after simulating on the
//! order of tens of millions of operations. Since the keys in the storage are meaningless hashes,
//! there is currently no good way to prune the history to reduce memory usage. This poses some technical
//! problems for long-running venues, though I don't believe they are insurmountable.
//!
//! ### Models, Submodels, and Encapsulation
//!
//! In Peregrine, a model is simply a set of resources. They can be resources that the model declares,
//! resources that it imports, or resources from composed submodels. Since resource labels use Rust's
//! type system, they can be encapsulated in a submodel by declaring it with private visibility, or
//! exposed to other submodels with public visibility. This design has a few benefits:
//! - Easier modularity for levels-of-fidelity. If two models are nearly the same, except one uses
//!   higher fidelity submodel for one subsystem, all the activities that *don't* touch that subsystem
//!   are trivially applicable to both models.
//! - Shared history between models that share resources. History is recorded by resource, not
//!   by model or plan. If the same state appears in different plans on different models, the history
//!   can still be reused.
//! - Encouraging concurrent logic. By making it easy to encapsulate hidden behavior inside submodels,
//!   developers will (hopefully) write models that are naturally more concurrent by virtue of separation
//!   of concerns.
//!
//!
//! ## Quick-start
//!
//! First, you need to declare a model and some resources to operate on. For that, use the [model] macro.
//!
//! ```
//! # fn main() {}
//! use peregrine::model;
//!
//! model! {
//!     ExampleModel {
//!         // These are two private resources, only accessible to activities defined
//!         // in this Rust module.
//!         sol_counter: u32,
//!         downlink_buffer: Vec<String>
//!     }
//! }
//! ```
//!
//! This creates two resources and a model that uses them. You can also declare resources divorced
//! from any model with the [resource][resource!] macro, and then import them into models.
//! See the [model] and [resource][resource!] macros for more details on how to call them.
//! Next, we can make activities that increment the sol and log the current sol to the buffer:
//!
//! ```
//! # fn main() {}
//! # use serde::{Serialize, Deserialize};
//! # use peregrine::anyhow::Result;
//! # resource!(sol_counter: u32);
//! # resource!(downlink_buffer: Vec<String>);
//! use peregrine::*;
//!
//! #[derive(Hash, Serialize, Deserialize)]
//! struct IncrementSol;
//!
//! # #[typetag::serde]
//! impl Activity for IncrementSol {
//!     fn run(&self, mut ops: Ops) -> Result<Duration> {
//!         ops += op! {
//!             // This is syntactic sugar for a read-write operation on the sol_counter
//!             // resource. Resources can be accessed as read-only with `ref:`, and write-only
//!             // with `mut:`.
//!             ref mut: sol_counter += 1;
//!         };
//!         // Return statement indicates the activity had zero duration and no errors
//!         Ok(Duration::ZERO)
//!     }
//! }
//!
//! #[derive(Hash, Serialize, Deserialize)]
//! struct LogCurrentSol {
//!     // Verbosity is taken in as an activity argument.
//!     verbose: bool,
//! }
//!
//! # #[typetag::serde]
//! impl Activity for LogCurrentSol {
//!     fn run(&self, mut ops: Ops) -> Result<Duration> {
//!         let verbose = self.verbose;
//!         ops += op! {
//!             // You can access activity arguments both inside and outside operations.
//!             if verbose {
//!                 ref mut: downlink_buffer.push(format!("It is currently Sol {}", ref:sol_counter));
//!             } else {
//!                 ref mut: downlink_buffer.push(format!("Sol {}", ref:sol_counter));
//!             }
//!         };
//!         Ok(Duration::ZERO)
//!     }
//! }
//! ```
//!
//! Next, you need to create a session and plan. You'll typically only have one session object
//! at a time, but can have multiple active plans running in it.
//!
//! ```
//! # use std::str::FromStr;
//! # use peregrine::{initial_conditions, model};
//! # model! {
//! #     ExampleModel {
//! #         sol_counter: u32,
//! #         downlink_buffer: Vec<String>
//! #     }
//! # }
//! use hifitime::TimeScale;
//! use peregrine::{Session, Time};
//!
//! let session = Session::new();
//!
//! let start_time = Time::from_day_of_year(2025, 31.0, TimeScale::TAI);
//! let mut plan = session.new_plan::<ExampleModel>(
//!     start_time,
//!     initial_conditions! {
//!         sol_counter: 1000,
//!         downlink_buffer, // Falls back on the default, `vec![]`
//!     },
//! );
//! ```
//!
//! Now we're ready to add some activities and simulate!
//!
//!
//! ```
//! # use std::str::FromStr;
//! # use hifitime::TimeScale;
//! # use peregrine::*;
//! # use serde::{Serialize, Deserialize};
//! # use anyhow::Result;
//! # model! {
//! #     ExampleModel {
//! #         sol_counter: u32,
//! #         downlink_buffer: Vec<String>
//! #     }
//! # }
//! # #[derive(Hash, Serialize, Deserialize)]
//! # struct IncrementSol;
//! # #[typetag::serde]
//! # impl Activity for IncrementSol {
//! #     fn run(&self, mut ops: Ops) -> Result<Duration> {
//! #         ops += op! {
//! #             ref mut: sol_counter += 1;
//! #         };
//! #         Ok(Duration::ZERO)
//! #     }
//! # }
//! # #[derive(Hash, Serialize, Deserialize)]
//! # struct LogCurrentSol {
//! #     verbose: bool,
//! # }
//! # #[typetag::serde]
//! # impl Activity for LogCurrentSol {
//! #     fn run(&self, mut ops: Ops) -> Result<Duration> {
//! #         let verbose = self.verbose;
//! #         ops += op! {
//! #             if verbose {
//! #                 ref mut: downlink_buffer.push(format!("It is currently Sol {}", ref:sol_counter));
//! #             } else {
//! #                 ref mut: downlink_buffer.push(format!("Sol {}", ref:sol_counter));
//! #             }
//! #         };
//!         Ok(Duration::ZERO)
//!     }
//! }
//! # use peregrine::{Session, Time};
//! # fn main() -> Result<()> {
//! # let session = Session::new();
//! # let start_time = Time::from_day_of_year(2025, 31.0, TimeScale::TAI);
//! # let mut plan = session.new_plan::<ExampleModel>(
//! #     start_time,
//! #     initial_conditions! {
//! #         sol_counter: 1000,
//! #         downlink_buffer,
//! #     },
//! # );
//! use hifitime::TimeUnits;
//!
//! plan.insert(start_time + 1.days(), IncrementSol)?;
//! plan.insert(start_time + 2.days(), IncrementSol)?;
//!
//! plan.insert(start_time + 1.hours(), LogCurrentSol { verbose: true })?;
//! plan.insert(start_time + 1.days() + 1.hours(), LogCurrentSol { verbose: false })?;
//!
//! assert_eq!(
//!     Some("It is currently Sol 1000"),
//!     plan.sample::<downlink_buffer>(start_time + 12.hours())?.last()
//! );
//! assert_eq!(
//!     Some("Sol 1001"),
//!     plan.sample::<downlink_buffer>(start_time + 1.days() + 12.hours())?.last()
//! );
//! # Ok(())
//! # }
//! ```
//!
//! Behind the scenes, even though `sample` triggered two different simulations, each of the four
//! activities was only executed once.
//!
//! ## Timekeeping & Ephemera
//!
//! Peregrine uses [hifitime](https://docs.rs/hifitime/latest/hifitime/) for timekeeping. The [Epoch][Time]
//! type, renamed in Peregrine to [Time] for simplicity, is used to order operations and activities.
//! The [Duration] type represents difference between [Time]s. As for why I chose hifitime, this line
//! from their documentation should explain it:
//!
//! > This library is validated against NASA/NAIF SPICE for the Ephemeris Time to Universal
//! > Coordinated Time computations: there are exactly zero nanoseconds of difference between
//! > SPICE and hifitime for the computation of ET and UTC after 01 January 1972.
//!
//! There is a significant performance penalty with this library when constructing large plans, due to
//! its non-trivial comparison and ordering. I believe its worth it for compatibility with SPICE,
//! and the penalty isn't present during simulation anyway.
//!
//! As for SPICE itself, there is unfortunately no official Rust SPICE library. There are two other
//! options though: the [rust-spice](https://crates.io/crates/rust-spice) and [ANISE](https://crates.io/crates/anise)
//! crates. `rust-spice` provides Rust bindings to CSPICE; some functions have idiomatic wrappers,
//! and the rest require you to use raw unsafe C primitives. `ANISE`, made by the same creators of
//! `hifitime`, is a rust-based multithreaded rewrite of SPICE's core functions, and is validated against
//! SPICE itself. For some projects SPICE support is a dealbreaker, but I think its worth checking
//! these out first.
//!
//! ## Current Features
//!
//! - **Incremental Simulation;** simulating only operations downstream of plan changes.
//! - **Targeted Simulation;** simulating only operations upstream of requested resource time-ranges.
//! - **Parallel Execution;** using [rayon](https://crates.io/crates/rayon) to execute the plan
//!   with as much parallelism as the DAG and your CPU will allow.
//! - **Session History;** any state encountered in the session previously - for any plan, model, or
//!   even the current simulation - can be reused for incremental updates.
//! - **Activity Spawning;** Activities can spawn sub-activities when inserted into the plan. See
//!   the [impl_activity] macro for details.
//! - **Model Composition;** You can create submodels, including ones that share common resources
//!   with each other, and then combine them into full models. Plans that work on the submodels can
//!   be combined into the full model.
//! - **Dynamic Delay in Operations;** Operations can have delays that are determined during simulation
//!   rather than plan construction. This allows for more flexible scheduling where the placement of an
//!   operation depends on the current state of resources.
//! - **Generalized Dynamic Resources;** The [Data] trait allows you to produce arbitrary functions
//!   from a single operation. This improves quality of life and enables hypotheticals. Currently I've
//!   implemented [polynomials][resource_types::polynomial::Polynomial] and [piecewise functions][resource_types::piecewise::Piecewise].
//! - **Timekeeping Builtins;** the [now][resource_types::builtins::now] and [elapsed][resource_types::builtins::elapsed]
//!   resources are automatically provided to all plans.
//! - **Its also just really fast in general;** Even in peregrine's worst case (a linear DAG on a
//!   cheap model, with no past simulations or repeating state), it still outperforms Merlin significantly.
//!
//! ## Possible Features
//!
//! This project is currently a proof-of-concept, but I've set it up with future development in mind.
//! These features could be implemented if there was demand:
//! - **Stateful activities;** activities that store an internal state as a transient resource that they
//!   bring to the model
//! - **Daemon tasks;** background tasks associated with the model that can either generate a statically-known
//!   set of recurring operations, or create "responsive" operations that are placed immediately after
//!   any other operation writes to a given resource.
//! - **Linked lists in history;** the above example of accumulating a `Vec<String>` buffer in a resource
//!   is *extremely* inefficient. For every operation that writes to it, the vector will be cloned,
//!   leading to quadratic runtime and memory usage. It is possible but non-trivial to make a linked
//!   list that lives inside the history hashmap and persists through serialization. (In reality it
//!   would be an n-ary tree that branches according to changes in the plan, but for any given simulation
//!   it would appear to be a linked list.)
//! - **Look-back reads;** currently operations can only read the current value of resources when
//!   they happen, but there's no reason why they shouldn't be able to look back to a pre-determined
//!   time.
//! - **Activity anchoring;** activities could be defined relative to other activities, as long as the
//!   relationship is known ahead-of-time.
//! - **Probabilistic Caching;** if the overhead of reading/writing history is a problem, I could
//!   potentially do pseudo-random caching (such as "only cache if `hash % 10 == 0`") without a large penalty
//!   to cache misses.
//!
//! ## Impossible Features
//!
//! Peregrine has to impose some restrictions on your activities and operations, so some things are
//! impossible:
//! - **Hidden state;** all state in the simulation must be recorded by the history. Getting around
//!   this restriction is UB.
//! - **Non-reentrant or non-deterministic activities;** the engine assumes that for the same input,
//!   all operations will produce the same output, and if a cached value exists in history then it is valid.
//!   It also assumes that it is OK to only resimulate a portion of an activity's operations.

// Public API - what users should import
pub mod public;

// Internal implementation - not for users
#[doc(hidden)]
pub mod internal;

// Re-export public types for convenience
pub use anyhow;
pub use hifitime;
pub use hifitime::{Duration, Epoch as Time};
pub use peregrine_macros::{Data, MaybeHash, delay, model, op};
pub use public::{
    Model,
    activity::*,
    plan::*,
    resource::{builtins::*, piecewise::*, polynomial::*, timer::*, *},
    session::*,
};
