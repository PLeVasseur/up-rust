# SDK safety manual — draft integrator guidance

This manual describes the 17 SDK assumptions of use of the up-rust SEooC
specification in this repository (`up-spec/safety/seooc-up-rust`), one entry each.
It is a review proposal, not a certified release, safety case, assessment,
deployment approval or implementation claim. Each entry quotes its entire
normative statement, then separates integrator actions, violation manifestations
and detection limits. The original AsciiDoc inline notation is retained inside
the quotation for exact comparison.

These entries matter to an integrator that claims the element's safety-assurance
scope for a configuration. Ordinary use, which claims no such scope, needs none of
these procedures: the SDK's functional communication, deadline monitoring, honest
reports and native resource rules apply without them. Functional support, native
resource behavior, a finite timing bound and safety assurance are independent
claims, and no OS family, architecture, compiler or middleware confers a timing
bound or an ASIL. An explicitly required claim is admitted under its supported
configuration or refused at setup by name. A manual states an integration
contract; it is not a safety case or a certificate.

The specification describes behavior still under development. Strict mode,
Enforce/Locked checks and reports, deployment integrity checking, supported
configuration packages and final Streamer behavior are not established as
available by this SDK baseline or this manual. Enforce and Locked are usable
before sign-off: they check what a configuration declares, refuse an assurance
scope only where one is explicitly required and unsupported, and report the rest
as Draft and unassessed; an ASIL label is not such a request. Do not rely on their quoted refusals, counters or warnings until the
implementing release is qualified on the deployment target. The SDK's end-to-end
protection wire is deferred and not part of this delivery: its requirements keep
their identifiers and revisions, the quotations that describe its use bind only
once it is delivered, and the integrator's own protection is the only mechanism
available now. The e2e-protection entry, revision 7, keeps the duties that hold
now, deciding, supplying and justifying protection and the black-channel and
decomposition rules, and confines the wrapper's declarations, accounting and
strict RPC rules to a route that selects it. Runtime SDK and binding code must
neither allocate nor free heap memory on a native path or in the work it causes.
A serialized route may allocate only as it declares and setup admits, after the
refusals that can come first, and releases each owned value it admitted exactly
once when that value's ownership ends, by release, refusal, overflow,
cancellation, expiry or a peer's failure. Deciding, reporting and counting those
outcomes uses no heap. An owning type, wire, binding or provider is admitted only
where destroying and releasing what it owns is established not to panic or abort,
on whichever thread does it; otherwise setup refuses it by name. A destruction
or retained return that could wait is moved off the drop that ends it to the
route's preallocated, permanent release service, which moves only the wait: it
stays pending, charged and isolated from public operations, deadline and health
accounting and other owners' returns, with no completion deadline promised, and
setup never relies on it to dispose of a refusal. Nothing releases runtime
infrastructure. A declared heap class makes
allocation visible and waives nothing, and a middleware waiver cannot relax the
native duty.

Timing claims rest on a supported configuration's package. The deadline monitor's
detection bound is D + 2L + resolution + J from the last true arrival to the
winning counter update and its visibility; an application's later read is outside
J. No configuration establishes a finite J yet, so no entry here supplies one.
Duties tied to narrowed values and to the one-word tick monitor apply only where a
configuration keeps those representations; at this revision no supported profile
does, host or MCU, as the narrowed-value requirement records. A logical deadline decides an outcome,
not when a descheduled caller sees it; physical return, observation and reaction
bounds are separate claims.

The revisions quoted here rose where meaning changed: supervisor-restart 4,
process-isolation 3, integrity-levels 5, timing-budgets 6, clock 3,
e2e-protection 7, qualified-bindings-and-wires 5, target-environment 4,
build-configuration 5, capacity-sizing 3, application-reacts 4 and
integrator-platform 4. The target-environment entry is new; earlier versions of
this folder listed it without providing it. The peer-access and peer-rates
statements changed only the revisions of the items they cite, and the
deployment-data entry's own guidance changed while its assumption did not. The
build-configuration, capacity-sizing and qualified-bindings-and-wires entries
were corrected in place at the revisions above. The remaining entries are unchanged.
Quoting a raised revision grants no product coverage and moves no code or test
tag: code tagged with an earlier revision of a requirement still needs review
against the raised one.

An event-only route has no deadline-based silence/delay detection. An aggregate
route deadline cannot detect a starved peer while another keeps arriving. Zero
observable discard counts do not establish that middleware lost nothing. A refused
RPC copy gives its claim on the call back, and the call stays open; only absence of
a later acceptable response or protected final answer makes TTL the completion
outcome. A call's outcome never waits for the release of what it owned, and its
slot is reused only once nothing it holds is pending.

The Streamer owns monitor-thresholds and operator-access, both revision 2 in this
specification. Their manuals belong in its repository; this folder has no
coverage items for them. References to Streamer behavior in SDK assumptions do
not transfer ownership or close Streamer implementation work.

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

The quoted binding records retain those categories, including the separately
fixed expired-buffer failure. Their presence is not certification or fresh
verification. Consult the binding's own published README/records for its exact
version. The SDK manual cannot replace a binding README.

No coverage is claimed here for the complete async seam, atomic windows,
message-ID residual derivation, values-only deployment schema, exhaustive
platform classification or binding README requirements. A checklist is not
the missing implementation contract, schema or complete analysis. Rust-specific
guidance for memory providers, the optional allocation and host layers,
application roles and the asynchronous layer is in `docs/component-guidance/`;
it is implementation guidance, not an assumption of use or a coverage entry.

Run `python3 tools/ci/check_safety_manual.py` from the SDK checkout after every
specification change; its unit tests build their own small fixture
specification, whose revisions are fixture values rather than citations of this
one. The committed coverage configuration keeps the seventeen SDK assumptions in
its gating run through their gate tag and adds `SafetyManual` in its report-only
run. The checker compares the exact quoted statements and revisions
and enforces the 17/2 ownership split. It cannot judge the adequacy of an
integrator's evidence or certify the product. Its supported Markdown/AsciiDoc
grammar is deliberately narrow and fails on unrecognized item markup.

Public specification: [uProtocol up-spec](https://github.com/eclipse-uprotocol/up-spec).
Normative-keyword convention: [IETF BCP 14](https://www.rfc-editor.org/info/bcp14).
No private assessment or licensed standard text is reproduced as guidance.

## Entries

- [supervisor-restart](supervisor-restart.md)
- [process-isolation](process-isolation.md)
- [integrity-levels](integrity-levels.md)
- [peer-access](peer-access.md)
- [timing-budgets](timing-budgets.md)
- [clock](clock.md)
- [waiver-approval](waiver-approval.md)
- [e2e-protection](e2e-protection.md)
- [qualified-bindings-and-wires](qualified-bindings-and-wires.md)
- [target-environment](target-environment.md)
- [build-configuration](build-configuration.md)
- [deployment-data](deployment-data.md)
- [capacity-sizing](capacity-sizing.md)
- [application-reacts](application-reacts.md)
- [integrator-platform](integrator-platform.md)
- [peer-rates](peer-rates.md)
- [build-order](build-order.md)
