# SDK safety manual — draft integrator guidance

This manual describes the 19 assumptions of use of the up-rust SEooC
specification in this repository (`up-spec/safety`) that the SDK owns, one entry
each.
It is a review proposal, not a certified release, safety case, assessment,
deployment approval or implementation claim.

## Where the specification states each entry

The specification files each assumption by the protocol layer its main duty
concerns, and indexes all of them in
`up-spec/safety/seooc-up-rust/assumptions-of-use.adoc`.

| Part of the specification | Entries |
| --- | --- |
| Every transport, `up-spec/safety/up-l1/deployment.adoc` | peer-access, integrity-levels, qualified-bindings-and-wires |
| The element and its deployment, `up-spec/safety/seooc-up-rust/assumptions-of-use.adoc` | the other sixteen |

- Every entry is an assumption of the up-rust SEooC wherever it is filed, and
  binds the same integrator. Filing one with a layer makes it no duty of another
  SDK or transport.
- A duty that binds only one transport, or only some deployments, is an
  assumption of its own. Four assumptions of the element are owned by another
  repository and have no entry here: see "Assumptions owned elsewhere" below.
- Where an assumption still names a rule of another part, the specification's
  table beside it says which part that is and who acts.
- A deployment duty and a software requirement stay separate. This manual states
  deployment duties; the requirements they rest on are the specification's.

## How each entry is built

Each entry quotes its entire normative statement, then separates integrator
actions, violation manifestations and detection limits.
The original AsciiDoc inline notation is retained inside the quotation for exact
comparison.
A reference to another requirement or assumption appears there as an AsciiDoc
link, `xref:<file>#<anchor>[<title>]` or `<<anchor,title>>`. The title is that
item's heading in the specification, and a file path is relative to the
specification file that states the quoted assumption.

The specification defines the obligations, and this manual traces to it.
Where the manual and the specification differ, the specification governs.

## Who needs these entries

These entries matter to an integrator that claims the element's safety-assurance
scope for a configuration.

Ordinary use, which claims no such scope, needs none of these procedures: the
SDK's functional communication, deadline monitoring, honest reports and native
resource rules apply without them.

Four claims are independent:

- functional support;
- native resource behavior;
- a finite timing bound;
- safety assurance.

No OS family, architecture, compiler or middleware confers a timing bound or an
ASIL.
An explicitly required claim is admitted under its supported configuration or
refused at setup by name.
A manual states an integration contract; it is not a safety case or a certificate.

A serialized route may also require four resource guarantees, each by name:
panic-free cleanup, non-waiting acquisition, non-waiting release and recoverable
exhaustion.
Ordinary use of a wire that owns its values, heap-backed ones included, requires
none of them.

## What is not established yet

The specification describes behavior still under development.

- Strict mode, Enforce/Locked checks and reports, deployment integrity checking,
  supported configuration packages and final Streamer behavior are not established
  as available by this SDK baseline or this manual.
- Enforce and Locked are usable before sign-off: they check what a configuration
  declares, refuse an assurance scope only where one is explicitly required and
  unsupported, and report the rest as Draft and unassessed; an ASIL label is not
  such a request.
- Do not rely on their quoted refusals, counters or warnings until the
  implementing release is qualified on the deployment target.

### The end-to-end protection wire is deferred

The SDK's end-to-end protection wire is deferred and not part of this delivery.

- Its requirements keep their identifiers and revisions.
- The quotations that describe its use bind only once it is delivered.
- The integrator's own protection is the only mechanism available now.
- The e2e-protection entry, revision 7, keeps the duties that hold now: deciding,
  supplying and justifying protection, and the black-channel and decomposition
  rules.
- It confines the wrapper's declarations, accounting and strict RPC rules to a
  route that selects it.

## Heap, destruction and release rules

**Native paths.**
Runtime SDK and binding code must neither allocate nor free heap memory on a
native path or in the work it causes.

**Serialized routes.**

- A serialized route may allocate only as it declares and setup admits, after the
  refusals that can come first.
- It releases each owned value it admitted exactly once when that value's
  ownership ends, by release, refusal, overflow, cancellation, expiry or a peer's
  failure.
- Deciding, reporting and counting those outcomes uses no heap.

**Ordinary ownership.**

- A serialized route that declares its allocation carries values that own what
  they hold, heap memory included. Setup does not refuse a type for having a
  destructor, for owning heap memory or for lacking a stronger guarantee.
- A destructor is not evidence of heap use, of waiting or of a panic. A wire's
  heap class for its decode direction covers a decoded value's initialization,
  replacement and destruction.
- On a route that requires nothing more, a value is destroyed where its ownership
  ends, under the provider's declared policy. That destruction can wait behind
  the allocator's lock, an allocator can end the process on exhaustion, and
  nothing is promised about a destructor's panics. The route's report says so.
  Each of these follows its own guarantee: requiring one changes nothing for the
  others.

**Guarantees a route requires by name.**

- Panic-free cleanup, non-waiting acquisition, non-waiting release and
  recoverable exhaustion are separate guarantees, and each limits its own
  operations only. One that a route requires is admitted only where evidence
  establishes it for the exact type, wires, binding, provider, build and
  contexts. Otherwise setup refuses the route by name.
- A requirement comes from the route's declaration, or from a timing or assurance
  claim whose package depends on it. An integrity level, an ASIL label, a
  strict-mode state and a name are neither a requirement nor evidence.
- A guarantee that a route does not require is not admitted for it and refuses
  nothing. Whether it is established is reported separately, from the evidence
  alone: established where the evidence exists, even unrequired, and not
  established only where the evidence is absent or insufficient. An item that
  relies on a guarantee has to require it.

**A destruction or return that could wait, where non-waiting release is required.**

- It is moved off the drop that ends it to the route's preallocated, permanent
  release service, which moves only the wait.
- It stays pending, charged and isolated from public operations, deadline and
  health accounting and other owners' returns, with no completion deadline
  promised.
- A configuration without a release service offers no non-waiting release over a
  dependency that can wait. That refuses a route that requires it and no other.
- Setup never relies on a release service to dispose of a refusal.

**What does not change.**

- Memory safety, exactly-once destruction and the rule that a slot is never
  reused while its value is owned hold on every route.
- Nothing releases runtime infrastructure.
- A declared heap class makes allocation visible. It relaxes no duty of the
  SDK's own code and reaches no native path.
- A middleware waiver cannot relax the native duty.

## Timing claims

Timing claims rest on a supported configuration's package.

- The deadline monitor's detection bound is D + 2L + resolution + J from the last
  true arrival to the winning counter update and its visibility; an application's
  later read is outside J.
- No configuration establishes a finite J yet, so no entry here supplies one.
- Duties tied to narrowed values and to the one-word tick monitor apply only where
  a configuration keeps those representations; at this revision no supported
  profile does, host or MCU, as the SDK's implementation traceability note
  records.
- A logical deadline decides an outcome, not when a descheduled caller sees it;
  physical return, observation and reaction bounds are separate claims.

## Revisions quoted here

The revisions quoted here rose where meaning changed:

| Entry | Revision |
| --- | --- |
| supervisor-restart | 4 |
| process-isolation | 3 |
| integrity-levels | 6 |
| capability-ceiling | 1 |
| peer-access | 5 |
| timing-budgets | 7 |
| clock | 3 |
| waiver-approval | 3 |
| e2e-protection | 7 |
| qualified-bindings-and-wires | 7 |
| resource-guarantees | 1 |
| deployment-data | 6 |
| target-environment | 4 |
| build-configuration | 5 |
| capacity-sizing | 3 |
| application-reacts | 4 |
| integrator-platform | 4 |

- The target-environment entry is new; earlier versions of this folder listed it
  without providing it.
- The peer-rates statement changed only the revisions of the items it cites.
- Filing the assumptions by layer changed no statement and no revision.
- The deployment-data entry's own guidance changed while its assumption did not.
- The build-configuration and capacity-sizing entries were corrected in place at
  the revisions above.
- The qualified-bindings-and-wires statement rose to revision 6: an integrator
  that relies on a stronger resource guarantee requires it by name.
- The application-reacts, build-configuration, target-environment and
  timing-budgets statements were reworded in place. They describe SDK interfaces
  instead of naming identifiers, and leave remarks about the present
  implementation to the traceability note. Their meaning and revisions are
  unchanged.
- The capacity-sizing statement cites a raised revision of the lease-drop
  requirement, with no other change.
- The remaining entries are unchanged.

The latest change separates responsibilities and changes no duty:

- peer-access (5), integrity-levels (6), qualified-bindings-and-wires (7),
  waiver-approval (3) and deployment-data (6) rose because clauses left them.
  Each keeps its remaining duties with their wording.
- capability-ceiling and resource-guarantees are new entries. Their text was part
  of integrity-levels 5 and of qualified-bindings-and-wires 6, and its wording is
  unchanged.
- The clauses that bind only a deployment on iceoryx2 are two assumptions of
  their own, which the iceoryx2 binding owns.
- What a faulty peer may do is defined once, in `up-spec/safety/README.adoc`.
  The peer-access entry names that definition instead of repeating it.
- The process-isolation, e2e-protection and target-environment statements
  changed only the items they cite.
- timing-budgets (7) rose for one clarification. Allowing for a middleware lock
  in a budget accepts no frame drop that waits for it: the drop hands its frame
  over without waiting, and only the return that follows may wait, as pending
  cleanup. The entry's other clauses are unchanged.
- A violation that the specification only leaves unchecked is described as that,
  and not as undetectable. The iceoryx2 binding's level-separation entry uses
  this for a shared domain or OS user.

Quoting a raised revision grants no product coverage and moves no code or test
tag: code tagged with an earlier revision of a requirement still needs review
against the raised one.

## Detection limits that apply across entries

- An event-only route has no deadline-based silence/delay detection.
- An aggregate route deadline cannot detect a starved peer while another keeps
  arriving.
- Zero observable discard counts do not establish that middleware lost nothing.
- A refused RPC copy gives its claim on the call back, and the call stays open;
  only absence of a later acceptable response or protected final answer makes TTL
  the completion outcome.
- A call's outcome is decided before, and never changed by, the release of what
  it owned, and its slot is reused only once nothing it holds is unfinished.
- A route's report shows a resource guarantee as required or not, established or
  not, and admitted or not. The SDK cannot tell that an item relies on a
  guarantee that no route requires.

## Assumptions owned elsewhere

Four assumptions of the element are stated in another repository's safety manual.
This folder has no coverage items for them.

**The Streamer** owns monitor-thresholds and operator-access, both revision 2 in
this specification, which states them with the Streamer's requirements in
`up-spec/safety/up-l2/streamer.adoc`.

- Their manuals belong in its repository.
- References to Streamer behavior in SDK assumptions do not transfer ownership or
  close Streamer implementation work.

**The iceoryx2 binding** owns iceoryx2-level-separation and iceoryx2-limits, both
revision 1, which the specification states in
`up-spec/safety/up-l1/iceoryx2.adoc`.

- They bind only a deployment on that binding: each integrity level in its own
  processes, domain and OS user, and that binding's limits, anomalies and known
  failures.
- Their manuals belong in the binding's repository.
- An integrator that deploys on iceoryx2 completes those two entries as well as
  the entries here. The duties of every transport stay in this manual.

## Three registers to keep distinct

Keep three registers distinct:

- Accepted named anomalies: reviewed limits with the exact ID and revision and
  deployment acceptance evidence. An acknowledgement is not a universal waiver.
- Open known failures: unresolved nonconformance; they cannot be accepted away
  by acknowledging an anomaly or adding a heap waiver. Include all affected
  publishers and subscribers when restricting deployment.
- Heap waivers: independently approved, version/deployment-bound permits for
  declared middleware allocation, with interference and heap-sizing arguments.
  They do not permit runtime heap activity in SDK/binding code on a native path
  or excuse panic.

The iceoryx2 binding's own entry quotes that binding's records in those
categories, including the separately fixed expired-buffer failure. This manual
no longer quotes them.
Their presence there is not certification or fresh verification.
Consult the binding's own published README/records for its exact version.
The SDK manual cannot replace a binding README.

## What this manual does not cover

No coverage is claimed here for the complete async seam, atomic windows,
message-ID residual derivation, values-only deployment schema, exhaustive
platform classification or binding README requirements.
A checklist is not the missing implementation contract, schema or complete
analysis.

Rust-specific guidance for memory providers, the optional allocation and host
layers, application roles and the asynchronous layer is in
`docs/component-guidance/`; it is implementation guidance, not an assumption of
use or a coverage entry.
That folder also holds the non-normative note that maps specification items to
the code that implements them.

## Checking the manual

Run `python3 tools/ci/check_safety_manual.py --spec-root up-spec/safety` from the
SDK checkout after every specification change.

- The assumptions are filed in three parts of `up-spec/safety`, so the checker
  reads that whole area, which is also its default root. A narrower root, such as
  `up-spec/safety/seooc-up-rust`, holds sixteen of the twenty-three and fails the
  ownership check.
- Its unit tests build their own small fixture specification. The fixture uses
  the identifiers and revisions that the checker pins, and its statements are
  fixture text, not this specification's.
- The committed coverage configuration keeps the nineteen SDK assumptions in its
  gating run through their gate tag and adds `SafetyManual` in its report-only
  run.
- The checker compares the exact quoted statements and revisions and enforces
  the ownership split: 19 entries here, and 4 assumptions owned elsewhere, 2 by
  the Streamer and 2 by the iceoryx2 binding.
- It pins the revision of each of the 23 assumptions, the revision of each
  entry's own item and each assumption's tags. A change to any of them fails
  until the checker's ownership table is changed and reviewed with it.
- It also fails unless the specification declares the anchor of the faulty-peer
  definition exactly once: a line that is exactly `[[faulty-peer]]`, outside
  every statement. A line that starts with `//` is a comment and is not read
  for this, and a spelling directly after a backslash is escaped and ignored.
- It fails on the other declaration spellings that it recognizes: an inline
  `[[faulty-peer]]` or one with a label, `[[faulty-peer,...]]`; the shorthand
  `[#faulty-peer...]`; the macro `anchor:faulty-peer[...]`; and a leading `id`
  attribute, `[id=faulty-peer...]` or `[id="faulty-peer"...]`. A declaration
  inside a statement fails too. The matching is exact: with white space before
  or after it, the declaration line is no declaration and fails as another
  spelling.
- That list of spellings is fixed. The checker does not know every way AsciiDoc
  can assign an ID, such as an `id` attribute that follows another attribute.
  So it cannot rule out a second declaration in a form outside the list.
- It checks the iceoryx2 binding's two entries only when it is given that
  binding's manual directory, with
  `--owner-manual-root up-transport-iceoryx2-rust=<directory>`. Without that
  option it reports that owner's manual as `not_checked`, which is not a pass of
  that manual. It has no such option for the Streamer's two.
- A pass shows that the documents match the ownership table. It does not show
  that the ownership is right.
- It cannot judge the adequacy of an integrator's evidence or certify the product.
- Its supported Markdown/AsciiDoc grammar is deliberately narrow and fails on
  unrecognized item markup. Inside a statement as well as outside one, it fails
  on a line that is exactly `////`, `----`, `....` or `++++`, and on a line
  that starts with three backticks, `~~~`, `include::`, `ifdef::`, `ifndef::`,
  `ifeval::` or `endif::`. It does so before it compares any quotation, and it
  includes, evaluates and renders nothing.
- That grammar is a list of lines that fail, not a parser. It does not reject a
  delimiter line longer than four characters, a table, an example, sidebar or
  quotation block, a line comment inside a statement, an attribute entry or
  inline markup. So a pass says nothing about them.

## References

Public specification: [uProtocol up-spec](https://github.com/eclipse-uprotocol/up-spec).
Normative-keyword convention: [IETF BCP 14](https://www.rfc-editor.org/info/bcp14).
No private assessment or licensed standard text is reproduced as guidance.

## Entries

- [supervisor-restart](supervisor-restart.md)
- [process-isolation](process-isolation.md)
- [integrity-levels](integrity-levels.md)
- [capability-ceiling](capability-ceiling.md)
- [peer-access](peer-access.md)
- [timing-budgets](timing-budgets.md)
- [clock](clock.md)
- [waiver-approval](waiver-approval.md)
- [e2e-protection](e2e-protection.md)
- [qualified-bindings-and-wires](qualified-bindings-and-wires.md)
- [resource-guarantees](resource-guarantees.md)
- [target-environment](target-environment.md)
- [build-configuration](build-configuration.md)
- [deployment-data](deployment-data.md)
- [capacity-sizing](capacity-sizing.md)
- [application-reacts](application-reacts.md)
- [integrator-platform](integrator-platform.md)
- [peer-rates](peer-rates.md)
- [build-order](build-order.md)
