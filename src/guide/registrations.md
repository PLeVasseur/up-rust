# Bounded receive registrations

`transport-implementer-api` provides protocol-independent registration primitives.
`RegistrationConfig` reserves a positive queue capacity and selects DropNewest or
a bounded blocking policy. Pushes move items into that storage without growing it.
Overflow, deadline and closed errors are fixed-size values, with separate counters.
A blocking push blocks its delivering thread; a binding must qualify progress and
reentrant behavior with its actual runtime and storage budgets.

`Registration<T>` owns one inbox and one worker. `RegistrationHandler<T>` is a
generic async contract: its concrete future lives in the worker rather than in a
new boxed future for each item. These primitives have no payload, native identity
or frame dependencies. A typed transport supplies the admission handler later.

The binding supplies a detachment operation that removes its sink-table entry.
Admission closes first, waking blocked producers, before that operation takes the
binding's lock. This prevents a producer waiting for queue space from holding a
lock that detachment needs indefinitely. Bindings must not call handlers while
holding those locks.

`stop` closes admission, detaches, drains previously admitted items and waits for
the worker. Called from the worker's own handler, it detaches and returns the
nonterminal `RegistrationStopError::CalledFromWorker` instead of success; the
worker still finishes its current callback and drains the earlier admissions.
`stop_ref(&mut self)` borrows the owner: after a same-worker refusal or a
cancelled external call the join remains, so another task can retry and observe
the outcome. After an external stop completes, no further handler is invoked.

Dropping the registration closes admission and discards queued work. An external
drop aborts the worker; self-drop lets the current handler finish. Cancelling an
external `stop` future also drops the registration and aborts the worker;
cancelling `stop_ref` keeps it. Items retained independently by application code
remain owned; none of these operations revokes another owner's storage.
A worker that panics or is aborted closes its inbox and releases queued owners,
including when the worker was never polled. An external stop reports a handler
panic as `RegistrationStopError::HandlerPanicked` and an aborted worker as
`RegistrationStopError::Cancelled`; `is_terminal` is true for both and false for
`CalledFromWorker`. A retained application owner continues to keep its payload
alive.

The callback-oriented `ListenerAdmission` utility remains distinct from inbox
storage. Its asynchronous dispatch tracks nested callback context on each poll's
stack without allocating a context vector per dispatch. Context is removed at
yield/unwind and rebuilt on the polling thread; unrelated Tokio tasks do not
inherit it. Native callback guards remain thread-bound. Bindings using those
guards must initialize their thread-local context capacity during delivery-thread
setup; the generic registration path uses its setup-owned worker instead.

Logical queue capacity is not a transport's complete retained-frame budget.
The binding must also account for callbacks, blocked pushes and any pre-admission
worker state, and keep those within its declared storage limits.
