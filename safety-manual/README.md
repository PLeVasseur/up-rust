# SDK safety manual — draft integrator guidance

This manual describes 17 SDK assumptions of use in the corrected protocol-neutral
specification proposal. It targets SDK c2cf63033dc1d591e4369596e04c2f0ee7c8fb62
with corrected AoU text based on up-spec c0948dbd01df78275dcf50e01646b3265ccf0dc5;
no integrated replacement commit is claimed. It is a review proposal,
not a certified release, safety case, deployment approval or implementation claim.
Each entry quotes its entire normative statement, then separates integrator
actions, violation manifestations and detection limits. The original AsciiDoc
inline notation is retained inside the quotation for exact comparison.

The specification describes behavior still under development. Strict mode,
Enforce/Locked checks and reports, the E2E wire, the narrowed atomic design,
deployment integrity checking and final Streamer behavior are not established as
available by this SDK baseline or this manual. Do not rely on their quoted
refusals, counters or warnings until the implementing release is qualified on
the deployment target. Existing initialization/runtime modes do not imply that
all of these later mechanisms exist. Runtime SDK and binding code must neither
allocate nor free heap memory; a middleware waiver cannot relax that duty.

The process-isolation, capacity-sizing and peer-rates quotations now use revision
2. The first two retain their prior statement text with honest revision accounting;
peer-rates includes explicit individual-peer progress duties and observable-loss
limits. Timing-budgets revision 5, E2E-protection revision 6 and application-reacts
revision 3 quote the corrected wording without a second revision rise.
An event-only route has no deadline-based silence/delay detection. An aggregate
route deadline cannot detect a starved peer while another keeps arriving. Zero
observable discard counts do not establish that middleware lost nothing. A refused
RPC copy returns its slot; only absence of a later acceptable response or protected
final answer makes TTL the completion outcome.

The strict ASIL RPC rule now explicitly forbids re-adopting any incarnation older
than the newest adopted; an increasing-start declaration alone does not implement
that behavior. All E2E runtime behavior still needs implementation and qualification.
The pre-adoption never-arrived figure is exact only when retained distinct-arrival
history proves it; otherwise it explicitly reports unknown, distinct from zero.
Any cumulative figure with an unknown contribution remains incomplete. This
accepted requirement preserves bounded memory and recovery: history loss does
not stop traffic or permit runtime allocation, freeing or waiting. Count and
completeness must be read consistently, and saturation remains a separate limit.
The E2E-protection and application-reacts documentary entries now explain that
rule; their gate candidacy still requires the integrated documentary/OFT checks. No corrected quotation grants
product coverage or admission to the integrated stack.

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
  They do not permit runtime heap activity in SDK/binding code or excuse panic.

The quoted binding records retain those categories, including the separately
fixed expired-buffer failure. Their presence is not certification or fresh
verification. Consult the binding's own published README/records for its exact
version. The SDK manual cannot replace a binding README.

No coverage is claimed here for the complete async seam, atomic windows,
message-ID residual derivation, values-only deployment schema, exhaustive
platform classification or binding README requirements. A checklist is not
the missing implementation contract, schema or complete analysis.

Run `python3 tools/ci/check_safety_manual.py` from the SDK checkout after every
submodule change. The checker compares the exact quoted statements and revisions
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
