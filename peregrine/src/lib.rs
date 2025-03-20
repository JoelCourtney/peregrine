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
//! #             // This is syntactic sugar for a read-write operation on the sol_counter
//! #             // resource. Resources can be accessed as read-only with `ref:`, and write-only
//! #             // with `mut:`.
//! #             ref mut: sol_counter += 1;
//! #         };
//! #         // Return statement indicates the activity had zero duration and no errors
//! #         Ok(Duration::ZERO)
//! #     }
//! # }
//! # #[derive(Hash, Serialize, Deserialize)]
//! # struct LogCurrentSol {
//! #     // Verbosity is taken in as an activity argument.
//! #     verbose: bool,
//! # }
//! # #[typetag::serde]
//! # impl Activity for LogCurrentSol {
//! #     fn run(&self, mut ops: Ops) -> Result<Duration> {
//! #         let verbose = self.verbose;
//! #         ops += op! {
//! #             // You can access activity arguments both inside and outside operations.
//! #             if verbose {
//! #                 ref mut: downlink_buffer.push(format!("It is currently Sol {}", ref:sol_counter));
//! #             } else {
//! #                 ref mut: downlink_buffer.push(format!("Sol {}", ref:sol_counter));
//! #             }
//! #         };
//! #         Ok(Duration::ZERO)
//! #     }
//! # }
//! # use peregrine::{Session, Time};
//! # fn main() -> peregrine::Result<()> {
//! # let session = Session::new();
//! # let start_time = Time::from_day_of_year(2025, 31.0, TimeScale::TAI);
//! # let mut plan = session.new_plan::<ExampleModel>(
//! #     start_time,
//! #     initial_conditions! {
//! #         sol_counter: 1000,
//! #         downlink_buffer, // Falls back on the default `vec![]`
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
//! - **Dynamic Delay in Operations;** TODO
//! - **Generalized Dynamic Resources;** The [Data] trait allows you to produce arbitrary functions
//!   from a single operation. This improves quality of life and enables hypotheticals. Currently I've
//!   implemented [polynomials][resource::polynomial::Polynomial] and [piecewise functions][resource::piecewise::Piecewise].
//! - **Timekeeping Builtins;** the [now][resource::builtins::now] and [elapsed_time][resource::builtins::elapsed_time]
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

pub mod activity;
pub mod exec;
pub mod history;
pub mod macro_prelude;
pub mod operation;
pub mod reexports;
pub mod resource;
pub mod timeline;

/// Creates a model and associated structs from a selection of resources.
///
/// Expects a struct-like item, but without the `struct` keyword. For example:
///
/// ```
/// # fn main() {}
/// # use peregrine::{resource, model};
/// resource!(res_a: u32);
/// resource!(res_b: String);
/// model! {
///     MyModel {
///         res_a,
///         res_b
///     }
/// }
/// ```
///
/// This produces a vacant type named `MyModel` that can be used to instantiate a plan with the
/// selected resources (see [Session]).
///
/// ## Defining resources inline
///
/// You can also define the resources directly inside the model macro. This doesn't
/// mean the model "owns" the resources in some way; its just a shorthand to do exactly the same
/// thing as the above example.
///
/// ```
/// # fn main() {}
/// # use peregrine::model;
/// model! {
///     MyModel {
///         res_a: u32,
///         res_b: String
///     }
/// }
/// ```
///
/// ## Visibility
///
/// Just like the [resource][resource!] macro, you can set the visibility of both the model and any
/// resources defined within it.
///
/// ```
/// # fn main() {}
/// # use peregrine::model;
/// model! {
///     // Without `pub`, the model will be private to the module.
///     pub MyModel {
///         // This makes `res_a` a resource that can be shared and used by models and activities in other modules.
///         pub res_a: u32,
///
///         // `res_b` is private. Models and activities in other modules cannot touch it.
///         res_b: String
///     }
/// }
/// ```
///
/// This is useful to encapsulate behavior in a subsystem and define the resource interface that this
/// model provides to other submodels.
///
/// ## Submodels and Composition
///
/// You can split apart large models into smaller models that interface with each other. This helps with
/// separation of concerns, hopefully leading to more concurrent plans. It also allows you to make
/// plans on isolated submodels for testing or experimentation without dealing with the rest of the model.
///
/// ```
/// # fn main() {}
/// # use peregrine::{resource, model};
/// # use peregrine::resource::{polynomial::*, piecewise::Piecewise};
///
/// ////// in main.rs
///
/// // A common resource used by many submodels.
/// // In a real model an enum would be more appropriate than a String.
/// resource!(operating_mode: String);
///
/// model! {
///     PotatoSat {
///         // Not commonly used by submodels.
///         mission_phase: String,
///
///         // Submodel inclusions.
///         ..PowerSubsystem,
///         ..GrowthSubsystem
///     }
/// }
///
/// ////// in power.rs
///
/// model! {
///     pub PowerSubsystem {
///         // Primarily associated with power, but exposed
///         // to other subsystems.
///         pub heat_lamp_on: bool,
///
///         potato_charge: Piecewise<Linear>,
///         // ... other private resources not exposed
///
///         // Included from main
///         operating_mode
///     }
/// }
///
/// ////// in growth.rs
///
/// model! {
///     pub GrowthSubsystem {
///         sprouting_percent: Quadratic,
///         decay_percent: Linear,
///
///         // Only `potato_charge` is imported, not all of `PowerSubsystem`.
///         potato_charge,
///         operating_mode
///     }
/// }
/// ```
pub use peregrine_macros::model;
use std::cell::RefCell;

pub use peregrine_macros::op;

pub use crate::activity::{Activity, ActivityId};
use crate::activity::{DecomposedActivity, Placement};
use crate::exec::{ErrorAccumulator, ExecEnvironment};
pub use crate::history::History;
use crate::macro_prelude::{Data, GroundingContinuation};
use crate::operation::InternalResult;
use crate::operation::initial_conditions::InitialConditions;
use crate::resource::builtins::init_builtins_timelines;
use crate::timeline::{MaybeGrounded, Timelines, duration_to_epoch, epoch_to_duration};
pub use activity::Ops;
pub use anyhow::{Context, Error, Result, anyhow, bail};
use bumpalo_herd::Herd;
pub use hifitime::{Duration, Epoch as Time};
use oneshot::Receiver;
use operation::Continuation;
use parking_lot::RwLock;
use resource::Resource;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::RangeBounds;

#[derive(Default)]
pub struct Session {
    herd: Herd,
    history: RwLock<History>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_history(self) -> History {
        self.history.into_inner()
    }

    pub fn new_plan<'o, M: Model<'o> + 'o>(
        &'o self,
        time: Time,
        initial_conditions: InitialConditions,
    ) -> Plan<'o, M>
    where
        Self: 'o,
    {
        let mut history = self.history.write();
        M::init_history(&mut history);
        drop(history);
        Plan::new(self, time, initial_conditions)
    }
}

impl From<History> for Session {
    fn from(history: History) -> Self {
        Self {
            history: RwLock::new(history),
            ..Self::default()
        }
    }
}

/// A plan instance for iterative editing and simulating.
pub struct Plan<'o, M: Model<'o>> {
    activities: HashMap<ActivityId, DecomposedActivity<'o>>,
    id_counter: u32,
    timelines: Timelines<'o>,

    session: &'o Session,

    model: PhantomData<M>,
}

impl<'o, M: Model<'o> + 'o> Plan<'o, M> {
    /// Create a new empty plan from initial conditions and a session.
    fn new(session: &'o Session, time: Time, mut initial_conditions: InitialConditions) -> Self {
        let time = epoch_to_duration(time);
        let mut timelines = Timelines::new(&session.herd);
        init_builtins_timelines(time, &mut timelines);
        M::init_timelines(time, &mut initial_conditions, &mut timelines);
        Plan {
            activities: HashMap::new(),
            timelines,
            id_counter: 0,

            session,

            model: PhantomData,
        }
    }

    /// Reserve memory for a large batch of additional activities.
    ///
    /// Provides a noticeable speedup when loading large plans.
    pub fn reserve_activity_capacity(&mut self, additional: usize) {
        self.activities.reserve(additional);
    }

    /// Inserts a new activity into the plan, and returns its unique ID.
    pub fn insert(&mut self, time: Time, activity: impl Activity + 'static) -> Result<ActivityId> {
        let id = ActivityId::new(self.id_counter);
        self.id_counter += 1;
        let bump = self.session.herd.get();
        let activity = bump.alloc(activity);
        let activity_pointer = activity as *mut dyn Activity;

        let operations = RefCell::new(vec![]);
        let placement = Placement::Static(epoch_to_duration(time));
        let ops_consumer = Ops {
            placement,
            bump: &bump,
            operations: &operations,
        };

        let _duration = activity.run(ops_consumer)?;

        for op in &*operations.borrow() {
            op.insert_self(&self.timelines).unwrap();
        }

        self.activities.insert(
            id,
            DecomposedActivity {
                _time: time,
                activity: activity_pointer,
                operations: operations.into_inner(),
            },
        );

        Ok(id)
    }

    /// Removes an activity from the plan, by ID.
    pub fn remove(&mut self, id: ActivityId) -> Result<()> {
        let decomposed = self
            .activities
            .remove(&id)
            .ok_or_else(|| anyhow!("could not find activity with id {id:?}"))?;
        for op in decomposed.operations {
            op.remove_self(&self.timelines)?;
        }
        unsafe { std::ptr::drop_in_place(decomposed.activity) };

        Ok(())
    }

    /// Simulates and returns a view into a section of a resource's timeline. After creating a plan, call
    /// `plan.view::<my_resource>(start..end)?` to get a vector of times and values
    /// within the `start - end` range.
    ///
    /// This is the primary way of simulating the plan. There is no exposed API to simulate without
    /// requesting a specific view, or vice versa. Try to limit the requested range to only the times that you need.
    ///
    /// Dynamic resources might return results that include the time it was written twice (once in
    /// the returned tuple `(Time, R::Data::Read)` and again inside the `Read` type itself). These times
    /// should always be equal, and you are free to ignore one or the other.
    pub fn view<R: Resource>(
        &self,
        bounds: impl RangeBounds<Time>,
    ) -> Result<Vec<(Time, <R::Data as Data<'o>>::Read)>> {
        let mut nodes: Vec<MaybeGrounded<'o, R>> = self.timelines.range((
            bounds.start_bound().map(|t| epoch_to_duration(*t)),
            bounds.end_bound().map(|t| epoch_to_duration(*t)),
        ));

        let mut receivers: Vec<MaybeGroundedResult<R>> = Vec::with_capacity(nodes.len());
        let errors = ErrorAccumulator::default();

        enum MaybeGroundedResult<'h, R: Resource> {
            Grounded(
                Duration,
                Receiver<InternalResult<<R::Data as Data<'h>>::Read>>,
            ),
            Ungrounded(
                Receiver<InternalResult<Duration>>,
                Receiver<InternalResult<<R::Data as Data<'h>>::Read>>,
            ),
        }

        let timelines = &self.timelines;

        let history_lock = self.session.history.read();
        let history = unsafe { &*(&*history_lock as *const History).cast::<History>() };

        rayon::scope(|scope| {
            let env = ExecEnvironment {
                errors: &errors,
                history,
                stack_counter: 0,
            };
            for node in nodes.drain(..) {
                let (sender, receiver) = oneshot::channel();

                match node {
                    MaybeGrounded::Grounded(t, n) => {
                        receivers.push(MaybeGroundedResult::Grounded(t, receiver));
                        scope.spawn(move |s| {
                            n.request(Continuation::Root(sender), true, s, timelines, env.reset())
                        });
                    }
                    MaybeGrounded::Ungrounded(n) => {
                        let (grounding_sender, grounding_receiver) = oneshot::channel();
                        receivers.push(MaybeGroundedResult::Ungrounded(
                            grounding_receiver,
                            receiver,
                        ));
                        scope.spawn(move |s| {
                            n.request_grounding(
                                GroundingContinuation::Root(grounding_sender),
                                true,
                                s,
                                timelines,
                                env.reset(),
                            );
                            n.request(
                                Continuation::<R>::Root(sender),
                                true,
                                s,
                                timelines,
                                env.reset(),
                            );
                        });
                    }
                }
            }
        });

        if !errors.is_empty() {
            Err(errors.into())
        } else {
            receivers
                .into_iter()
                .map(|r| match r {
                    MaybeGroundedResult::Grounded(t, recv) => {
                        Ok((duration_to_epoch(t), recv.recv().unwrap()?))
                    }
                    MaybeGroundedResult::Ungrounded(t_recv, recv) => Ok((
                        duration_to_epoch(t_recv.recv().unwrap()?),
                        recv.recv().unwrap()?,
                    )),
                })
                .collect()
        }
    }

    /// Samples a resource at a given time.
    pub fn sample<R: Resource>(&self, time: Time) -> Result<<R::Data as Data<'o>>::Sample> {
        let (_, read) = self
            .view::<R>(time..=time)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("No operations to sample found at or before {time}"))?;
        Ok(R::Data::sample(&read, time))
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

impl<'o, M: Model<'o>> Serialize for Plan<'o, M> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let seq = serializer.serialize_seq(Some(self.activities.len()))?;
        for _activity in &self.activities {}
        seq.end()
    }
}

/// A selection of resources, with tools for creating a plan and storing history.
///
/// Autogenerated by the [model] macro. There is no point implementing this manually.
pub trait Model<'o>: Sync {
    fn init_history(history: &mut History);
    fn init_timelines(
        time: Duration,
        initial_conditions: &mut InitialConditions,
        timelines: &mut Timelines<'o>,
    );
}
