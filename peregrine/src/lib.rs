//! # Peregrine Engine
//!
//! A discrete event spacecraft simulation engine designed for schedulers.
//!
//! Peregrine always does the minimal amount of computation to respond to changes in the plan, and to
//! calculate only the requested resources *at the requested times*. If you only care about a couple
//! resources in the vicinity of a small plan change, then that's all the engine simulates.
//!
//! Peregrine also stores a permanent history of resource states, meaning that simulation is not just
//! incremental with respect to the most recent plan state; it is incremental with respect to all recorded
//! history. If you undo five recent simulated changes and add one new activity, the engine will only
//! simulate change of adding the activity, not of adding one and deleting five.
//!
//! Peregrine performs all simulation with as much parallelism as is mathematically allowed by the
//! configuration of the plan. Even on linear plan structures with no available concurrency, initial (extremely informal) benchmarking
//! suggests that Peregrine's engine overhead is significantly lower than Aerie-Merlin's - simulating
//! millions of simple operations per second instead of thousands. Highly expensive operations may
//! amortize the overhead differences, but will not amortize the parallelism.
//!
//! ## Concepts
//!
//! The engine simulates the evolution of a set of Resources over time, operated on by a
//! set of instantaneous Operations, which themselves are grouped together into Activities.
//!
//! ### Resources & Activities
//!
//! Resources are just variables whose evolution over time is tracked and recorded. Activities
//! contain operations that mutate those resources, and place those operations at specific
//! pre-determined times throughout a plan. This is the only fundamental difference between Peregrine and
//! Merlin; activities declare their operations - when they happen, what resources they read/write -
//! and their total duration ahead of time, before simulation.
//!
//! ### Operations, Dependencies, & Parallelism
//!
//! Operations are the instantaneous discrete events that the engine simulates. The can read and write
//! resources, access activity arguments, and be configured by the activity (ahead of time only).
//! By forcing you to declare which resources you read and write, the engine is able to build a
//! directed acyclic graph of operation dependencies. This DAG enables most of the parallelism and minimal
//! computation I bragged about in the intro. When you make a change and request a view of a resource,
//! the simulation propagates backward through the DAG from the requested range, and evaluation
//! of branches in the graph immediately stop when they find cached values from previous runs.
//!
//! ### History & Incremental Simulation
//!
//! Peregrine records the history of all operations that have been simulated. Currently, this is only
//! recorded per-session, but a persistent system could be implemented in the future. This enables
//! the engine to immediately stop as soon as it encounters a state that it has been in before. Importantly,
//! it recognizes the state using only the structure of the DAG and the initial conditions, not the
//! resource state at the time the operation was previously run. It does this by inductively calculating
//! hashes for each operation: each operation hashes together its own unique ID and the hashes of its dependencies,
//! and only the initial condition operations hash the input. This allows the engine to recognize past
//! states without performing any simulation.
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
//! ### Models
//!
//! For those familiar with Aerie-Merlin, you might notice that I didn't use the word "Model"
//! in the above descriptions. This is because while in Merlin, the model is a container that creates,
//! specifies, and owns its resources and activities, in Peregrine the model is just a selection of
//! pre-existing resources. Activities are applicable to any model that selects the resources it
//! operates on. This reinterpretation gives a couple advantages:
//! - Easier modularity for levels-of-fidelity. If two models are nearly the same, except one uses
//!   higher fidelity simulation for one subsystem, all the activities that *don't* touch that subsystem
//!   are trivially applicable to both models.
//! - Shared history between models that share resources. History is recorded by resource, not
//!   by model or plan. If the same sub-graph appears in different plans on different models, the history
//!   can still be reused.
//!
//!
//! ## Modelling quick-start
//!
//! First, you need resources to operate on. For each resource, you need a vacant type to act as a
//! label:
//!
//! ```
//! # fn main() {}
//! /// Counts the number of sols that have elapsed.
//! enum SolCounter {}
//! ```
//!
//! We need the type to act as a label because we need to differentiate between resources that are
//! represented with the same data type (which in this case will be `u32`). Next, implement the [Resource]
//! trait for the label:
//!
//! ```
//! # fn main() {}
//! # use peregrine::Resource;
//! # use peregrine::CopyHistory;
//! enum SolCounter {}
//!
//! impl<'h> Resource<'h> for SolCounter {
//!     const STATIC: bool = true;
//!     type Read = u32;
//!     type Write = u32;
//!     type History = CopyHistory<'h, SolCounter>;
//! }
//! ```
//!
//! See the [Resource] trait for more details on what these types mean and how to implement it. Say
//! our model has both the `SolCounter` resource and a `DownlinkBuffer` resource represented by a `Vec<String>`.
//! We can make an activity that logs the current sol to the buffer:
//!
//! ```
//! # fn main() {}
//! # use serde::{Serialize, Deserialize};
//! # use peregrine::{impl_activity, Resource, CopyHistory, DerefHistory, Duration};
//! # enum SolCounter {}
//! # impl<'h> Resource<'h> for SolCounter {
//! #     const STATIC: bool = true;
//! #     type Read = u32;
//! #     type Write = u32;
//! #     type History = CopyHistory<'h, SolCounter>;
//! # }
//! # enum DownlinkBuffer {}
//! # impl<'h> Resource<'h> for DownlinkBuffer {
//! #     const STATIC: bool = true;
//! #     type Read = &'h [String];
//! #     type Write = Vec<String>;
//! #     type History = DerefHistory<'h, DownlinkBuffer>;
//! # }
//! #[derive(Serialize, Deserialize)]
//! struct LogCurrentSol {
//!     /// Verbosity is taken in as an activity argument.
//!     verbose: bool,
//! }
//!
//! impl_activity! { for LogCurrentSol
//!     // This is syntactic sugar to declare an operation.
//!     // It occurs at time `start`, reads both `SolCounter` and `DownlinkBuffer`,
//!     // and writes to `DownlinkBuffer`.
//!     @(start) sol: SolCounter, buf: DownlinkBuffer -> buf {
//!         // Activity arguments are accessible under `args`, not `self`.
//!         if args.verbose {
//!             buf.push(format!("It is currently Sol {sol}"));
//!         } else {
//!             buf.push(format!("Sol {sol}"));
//!         }
//!     }
//!     Duration::ZERO // Return statement indicates the activity had zero duration
//! }
//! ```
//!
//! Lastly you need to make a model that uses these resources:
//!
//! ```
//! # fn main() {}
//! # use serde::{Serialize, Deserialize};
//! # use peregrine::{impl_activity, Resource, CopyHistory, DerefHistory, Duration, model};
//! # enum SolCounter {}
//! # impl<'h> Resource<'h> for SolCounter {
//! #     const STATIC: bool = true;
//! #     type Read = u32;
//! #     type Write = u32;
//! #     type History = CopyHistory<'h, SolCounter>;
//! # }
//! # enum DownlinkBuffer {}
//! # impl<'h> Resource<'h> for DownlinkBuffer {
//! #     const STATIC: bool = true;
//! #     type Read = &'h [String];
//! #     type Write = Vec<String>;
//! #     type History = DerefHistory<'h, DownlinkBuffer>;
//! # }
//! model! {
//!     DemoModel {
//!         sol: SolCounter,
//!         log: DownlinkBuffer
//!     }
//! }
//! ```
//!
//! This implements the [Model] trait, and generates structs to store initial conditions, [Plans][Plan],
//! and histories.
//!
//! ## Interaction
//!
//! TODO
//!
//! ## Timekeeping
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
//! ## Possible Features
//!
//! This project is currently a proof-of-concept, but I've set it up with future development in mind.
//! These features could be implemented if there was demand:
//! - **Stateful activities;** activities that store an internal state as a transient resource that they
//!   bring to the model
//! - **Daemon tasks;** background tasks associated with the model that can either generate a statically-known
//!   set of recurring operations, or create "responsive" operations that are placed immediately after
//!   any other operation writes to a given resource.
//! - **Maybe-reads and maybe-writes;** optimizations for operations that may or may not read or write a
//!   resource.
//! - **Global persistent history;** I made a lot of grand claims about sharing history between plans and
//!   models, but I haven't actually implemented that yet. Storing history on the filesystem is possible
//!   already though.
//! - **Stable graph hashing;** currently there are no guarantees that operations will generate the
//!   same hashes when the program is recompiled, but this could be fixed.
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
//! - **Activity spawning;** the activity body could automatically spawn child activities when inserted
//!   into the plan, as long as this spawning is only a function of the activity arguments.
//! - **Probabilistic Caching;** if the overhead of reading/writing history is a problem, I could
//!   potentially do pseudo-random caching (such as "only cache if `hash % 10 == 0`") without a large penalty
//!   to cache misses.
//!
//! ## Impossible Features
//!
//! Peregrine has to impose some restrictions on your activities and operations, so some things are
//! impossible:
//! - **Operation placement at runtime;** the exact placement of all activities and operations must
//!   be determined by only statically-known values like activity arguments and start time.
//! - **Hidden state;** all state in the simulation must be recorded by the history. Getting around
//!   this restriction is UB.
//! - **Non-reentrant or non-deterministic activities;** the engine assumes that for the same input,
//!   all operations will produce the same output, and if a cached value exists in history then it is valid.
//!   It also assumes that it is OK to only resimulate a portion of an activity's operations.

#![cfg_attr(feature = "nightly", feature(btree_cursors))]

use crate::exec::{ExecEnvironment, EXECUTOR, NUM_THREADS};
use bumpalo::boxed::Box as BumpBox;
pub use history::{CopyHistory, DerefHistory};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::RangeBounds;

/// Creates a model and associated structs from a selection of resources.
///
/// Expects a struct-like item, but without the `struct` keyword. For example:
///
/// ```
/// # fn main() {}
/// # use peregrine::{model, Resource, CopyHistory};
/// # enum ResourceA {}
/// # impl<'h> Resource<'h> for ResourceA {
/// #     const STATIC: bool = true;
/// #     type Read = u32;
/// #     type Write = u32;
/// #     type History = CopyHistory<'h, ResourceA>;
/// # }
/// # enum ResourceB {}
/// # impl<'h> Resource<'h> for ResourceB {
/// #     const STATIC: bool = true;
/// #     type Read = u32;
/// #     type Write = u32;
/// #     type History = CopyHistory<'h, ResourceB>;
/// # }
/// model! {
///     MyModel {
///         res_a: ResourceA,
///         res_b: ResourceB
///     }
/// }
/// ```
///
/// This generates a few types: a vacant `MyModel` type that implements `Model`, as well as
/// structs called `MyModelHistories` and `MyModelInitialConditions`. The initial conditions
/// are used to create a new plan, and has one field for each resource where you can populate
/// the resource's `Write` value. The histories are used to cache simulation results to be reused
/// in later simulations.
pub use peregrine_macros::model;

/// Implements the [Activity] trait for a type.
///
/// Expects a block of statements preceded by `for MyActivity`. The inside of the block is a function
/// that generates the activity's operations, and returns the duration of the activity. The start time
/// is accessible through the `start` variable, and the activity arguments are accessible through `args`.
///
/// The body of your activity function will contain operations that use a special syntactic sugar.
/// Let's break down this example:
///
/// ```
/// # fn main() {}
/// # use peregrine::{impl_activity, Resource, CopyHistory, DerefHistory, Duration};
/// use serde::{Serialize, Deserialize};
///
/// enum SolCounter {}
/// impl<'h> Resource<'h> for SolCounter {
///     const STATIC: bool = true;
///     type Read = u32;
///     type Write = u32;
///     type History = CopyHistory<'h, SolCounter>;
/// }
///
/// #[derive(Serialize, Deserialize)]
/// struct IncrementSol;
///
/// impl_activity! { for IncrementSol
///     @(start) sol: SolCounter -> sol {
///         sol += 1;
///     }
///     Duration::ZERO // Return statement indicates the activity had zero duration
/// }
/// ```
///
/// 1. First declare an empty struct `IncrementSol` to be our activity type. It has to
///    implement [Serialize], and [DeserializeOwned], and this is done through derive macros
///    provided by serde.
/// 2. Call [impl_activity] with the preamble `for IncrementSol`. Everything else inside the
///    macro is your function body. In this context, `start` is the start time of the activity,
///    and `args` are the arguments (in this case there are none).
/// 3. Declare operation by starting a statement with `@`.
///    - `(start)` indicates the time the operation happens at. It can be any valid rust expression
///      that evaluates to a [Duration].
///    - `sol: SolCounter` declares `SolCounter` as a resource read, available in the variable `sol`.
///    - `-> sol` declares that the `SolCounter` is also a resource write. The `: SolCounter` type
///      is implied, but you can write it explicitly if you want.
///    - You can read and write to as many resources as you want in one operation, just declare them
///      in a comma-separated list. Any write-only resources must have the explicit type tag.
///      (e.g. `sol: SolCounter, buffer: DownlinkBuffer, temp: Temperature -> buffer, safe_mode: SafeMode`
///      reads from `SolCounter` and `Temperature`, writes to `SafeMode`, and read-writes `DownlinkBuffer`.)
///    - The body of the operation can do whatever you want, as long as it is deterministic.
///      The body is also an async context; you could make a non-blocking web request if you want,
///      as long as it can be assumed to always return the same output for the same input.
/// 4. Finally, we end the activity body by returning `Duration::ZERO`, which means the activity took
///    zero duration.
///
/// It is *technically* valid to generate operations before the start time or after the declared end time.
/// It would just be very un-hygienic and potentially hard to debug.
pub use peregrine_macros::impl_activity;

pub mod exec;
pub mod history;
pub mod operation;
pub mod reexports;
pub mod timeline;
pub mod resource;

use operation::Operation;
pub use exec::SyncBump;
pub use hifitime::Duration;
pub use hifitime::Epoch as Time;
use timeline::HasTimeline;
use history::HistoryAdapter;
pub use history::History;

/// Marks a type as a resource label.
///
/// This trait is not applied to the actual data and is never instantiated, because multiple resources
/// might have the same representation. (i.e. both memory usage and battery state are f32's.) To enforce
/// that the resource type is only used as a label and never instantiated, it's recommended to make
/// it an empty enum (i.e. a vacant type).
///
/// Resources are not part of a model, the model is a selection of existing resources. This allows
/// activities, which are also not part of a model, to be applied to any model that has the relevant
/// resources.
///
/// ## Reading & Writing
///
/// Resources are not represented one data type, but two, one for reading and one for writing.
/// For simple [Copy] resources these two types will be the same, and you won't have to worry about it.
/// For more complex resources they may be different but related types, like [String] and [&str][str].
/// This is for performance reasons, to avoid unnecessary cloning of heap-allocated data.
///
/// The `Read` type is the input to an operation; it is what's read from history and from other operations.
/// The `Write` type is the output of an operation, and the actual type written and stored in the history.
/// Inside an operation, the type of the actual resource variable depends on how you use it.
/// - read-only: `Read`
/// - write-only: `Write`, initialized to the default value
/// - read and write: `Write`, initialized from the converted value read from history (likely cloned).
///
/// ## Choosing a history container
///
/// Currently, there are two types of storage for history ([CopyHistory] and [DerefHistory]), and
/// which you use depends on the properties of the type.
///
/// ### Copy
///
/// For a resource called `MyResource`, if the type written
/// from operations is [Copiable][std::marker::Copy], then you should use `CopyHistory<MyResource>`.
/// This requires that the `Read` and `Write` types are equal. For example:
///
/// ```
/// # fn main() {}
/// # use peregrine::Resource;
/// # use peregrine::CopyHistory;
/// enum SolCounter {}
///
/// impl<'h> Resource<'h> for SolCounter {
///     const STATIC: bool = true;
///     type Read = u32;
///     type Write = u32;
///     type History = CopyHistory<'h, SolCounter>;
/// }
/// ```
///
/// ### Deref
///
/// If the written type has a ["stable deref"][stable_deref_trait::StableDeref], meaning that it
/// dereferences to data whose address doesn't change even if the data is moved, then you can use [DerefHistory].
/// Common examples include [String], which dereferences to `str`; [`Vec<T>`], which derefs to `[T]`,
/// and [`Box<T>`], which derefs to `T`. All three of these types use heap allocations; so if you have
/// a [String], you can have an `&str` reference to its underlying data that remains valid even if
/// the [String] is moved (using a bit of unsafe code in the [elsa crate](https://docs.rs/elsa/latest/elsa/)).
///
/// If this describes your type, you can use `DerefHistory<MyResource>`, which requires that `Read = &*Write`.
/// (i.e. `Write = Vec<T>`, `Read = &[T]`, etc). For Example:
///
/// ```
/// # fn main() {}
/// # use peregrine::Resource;
/// # use peregrine::DerefHistory;
/// enum MissionPhase {}
///
/// impl<'h> Resource<'h> for MissionPhase {
///     const STATIC: bool = true;
///
///     // Note the `'h` lifetime specifier. This means that the reference
///     // lives as long as the history container itself.
///     type Read = &'h str;
///
///     type Write = String;
///     type History = DerefHistory<'h, MissionPhase>;
/// }
/// ```
///
/// ### Non-copy, non-deref types
///
/// If your type is neither copy nor stable-deref, currently the only option is to wrap it in a `Box`
/// and use `DerefHistory`. I may implement a `CloneHistory` in the future.
pub trait Resource<'h>: 'static + Sized {
    /// Whether the resource represents a value that can vary even when not actively written to by
    /// an operation. This is used for cache invalidation,
    /// so it is very important not to give false positives.
    ///
    /// The basic question is "does it matter
    /// how long it has been since this resource was last written to?" For a boolean resource, the
    /// answer is "no", and so it is static. But for a continuously-varying linear function,
    /// the answer is "yes"; the value it represents changes over time even in between operations.
    const STATIC: bool;

    /// The type that is read from history.
    type Read: 'h + Copy + Send + Sync + Serialize;

    /// The type that is written from operations to history.
    type Write: 'h
        + From<Self::Read>
        + Clone
        + Default
        + Debug
        + Serialize
        + DeserializeOwned
        + Send
        + Sync;

    /// The type of history container to use to store instances of the `Write` type, currently
    /// either [CopyHistory] or [DerefHistory]. See [Resource] for details.
    type History: 'static + HistoryAdapter<Self::Write, Self::Read> + Default + Send + Sync;
}

/// A plan session for iterative editing and simulating.
pub struct Plan<'o, M: Model<'o>> {
    activities: HashMap<ActivityId, DecomposedActivity<'o, M>>,
    bump: &'o SyncBump,
    id_counter: u32,
    timelines: M::Timelines,
}

struct DecomposedActivity<'o, M> {
    activity: *mut dyn Activity<'o, M>,
    operations: Vec<&'o dyn Operation<'o, M>>,
}

impl<'o, M: Model<'o> + 'o> Plan<'o, M> {
    /// Create a new empty plan from initial conditions.
    ///
    /// This function requires an instance of [SyncBump]. This is an arena allocator used to satisfy
    /// rust's borrowing rules without unsafe code or smart pointers. This is an unfortunate implementation detail
    /// as a result of Rust's borrow checker that makes it impossible to generate this object inside
    /// the `new` method. Just do this:
    ///
    /// ```
    /// // Dummy resource and model; replace with a real model.
    /// use peregrine::exec::SyncBump;
    /// use peregrine::{CopyHistory, Plan, Time};
    /// enum MyResource {}
    /// impl<'h> peregrine::Resource<'h> for MyResource {
    ///     const STATIC: bool = true;
    ///     type Read = ();
    ///     type Write = ();
    ///     type History = CopyHistory<'h, MyResource>;
    /// }
    /// peregrine::model! { MyModel { res: MyResource } }
    ///
    /// // Create a sync bump to be stored outside the plan.
    /// let bump = SyncBump::new();
    ///
    /// // Create a plan with the bump.
    /// let plan = Plan::<MyModel>::new(&bump, Time::now().unwrap(), MyModelInitialConditions { res: () });
    /// ```
    ///
    /// Rust's borrow checker will prevent you from moving or dropping `bump` before dropping the plan.
    pub fn new(bump: &'o SyncBump, time: Time, initial_conditions: M::InitialConditions) -> Self {
        Plan {
            activities: HashMap::new(),
            bump,
            timelines: (timeline::epoch_to_duration(time), bump, initial_conditions).into(),
            id_counter: 0,
        }
    }

    /// Inserts a new activity into the plan, and returns its unique ID.
    pub fn insert(&mut self, time: Time, activity: impl Activity<'o, M> + 'static) -> ActivityId {
        let id = ActivityId::new(self.id_counter);
        self.id_counter += 1;
        let activity = self.bump.alloc(activity);
        let activity_pointer = activity as *mut dyn Activity<'o, M>;
        let (_duration, operations) = activity.decompose(time, &self.timelines, self.bump);

        for op in &operations {
            op.insert_self(&mut self.timelines);
        }

        self.activities.insert(
            id,
            DecomposedActivity {
                activity: activity_pointer,
                operations,
            },
        );

        id
    }

    /// Removes an activity from the plan, by ID.
    pub fn remove(&mut self, id: ActivityId) {
        let decomposed = self.activities.remove(&id).unwrap();
        for op in decomposed.operations {
            op.remove_self(&mut self.timelines);
        }
        drop(unsafe { BumpBox::from_raw(decomposed.activity) });
    }

    /// Returns a view into a section of a resource's timeline. After creating a plan, call
    /// `plan.view::<MyResource>(start..end, &histories)` to get a vector of times and values
    /// within the `start - end` range.
    ///
    /// Try to limit the requested range to only the times that you need.
    ///
    /// The histories struct will be autogenerated by the [model] macro.
    pub fn view<R: Resource<'o>>(
        &self,
        bounds: impl RangeBounds<Time>,
        histories: &'o History,
    ) -> Vec<(Time, R::Read)>
    where
        M::Timelines: HasTimeline<'o, R, M>,
    {
        let bump = SyncBump::new();
        let nodes = self
            .timelines
            .get_operations((
                bounds
                    .start_bound()
                    .map(|t| timeline::epoch_to_duration(*t)),
                bounds.end_bound().map(|t| timeline::epoch_to_duration(*t)),
            ))
            .into_iter();
        let env = ExecEnvironment::new(&bump);
        std::thread::scope(move |scope| {
            // EXPLANATION:
            // The async executor crate provides an `executor.run(fut)` function,
            // that runs the executor until `fut` completes. Importantly, if `fut` yields,
            // the executor will keep doing other submitted tasks until `fut` wakes,
            // even if they are unrelated.

            // If `fut` is, say, awaiting an async shutdown signal, then the executor
            // will keep doing any other available tasks until the shutdown signal is received.
            // The following line creates that shutdown signal. It will be sent when
            // `_signal` goes out of scope, which will only happen after all the
            // tasks we actually care about are complete.
            let (_signal, shutdown) = async_channel::bounded::<()>(1);

            for _ in 0..NUM_THREADS {
                let shutdown = shutdown.clone();
                scope.spawn(move || futures::executor::block_on(EXECUTOR.run(shutdown.recv())));
            }

            futures::executor::block_on(futures::future::join_all(nodes.map(|(t, n)| async move {
                (
                    timeline::duration_to_epoch(t),
                    *n.read(histories, env).await.1,
                )
            })))
        })
    }
}

impl<'o, M: Model<'o>> Drop for Plan<'o, M> {
    fn drop(&mut self) {
        for decomposed in self.activities.values_mut() {
            unsafe {
                decomposed.activity.drop_in_place();
            }
        }
    }
}

/// A selection of resources, with tools for creating a plan and storing history.
///
/// Autogenerated by the [model] macro.
pub trait Model<'o>: Sync {
    type InitialConditions;
    type Timelines: Sync + From<(Duration, &'o SyncBump, Self::InitialConditions)>;

    fn init_history(history: &mut History);
}

/// An activity, which decomposes into a statically-known set of operations. Implemented
/// with the [impl_activity] macro.
pub trait Activity<'o, M: Model<'o>>: Send + Sync {
    fn decompose(
        &'o self,
        start: Time,
        timelines: &M::Timelines,
        bump: &'o SyncBump,
    ) -> (Duration, Vec<&'o dyn Operation<'o, M>>);
}

pub trait ActivityRemover<'o, M: Model<'o>> {
    fn remove(self, timelines: &mut M::Timelines);
}

/// A unique activity ID.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct ActivityId(u32);
impl ActivityId {
    pub fn new(id: u32) -> ActivityId {
        ActivityId(id)
    }
}
