# Historical follow-ups

Generated from exact versioned records. Current implementation and evidence are unassessed; applicability is unknown. Historical findings remain open observations, not current conformance verdicts. Transfers record pending filing only.

[Catalogue](README.md) · [Interactive view](index.html)

## RX03-FU-001

req~ssr-contract-transition~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/0/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-contract-transition~2",
  "component": "the supplier of each part: the up-rust SEooC, the Streamer, the iceoryx2 binding, the LoLa binding, each wire",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "No part's declaration (part, revision of the specification, named profiles) and no supplier's record of where the part implements each item that binds it has been assessed against this revision; a build result or a report is no claim.",
  "directive": "XR-03; SR-283; SR-368",
  "cluster": "C01-governing"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/0/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/0",
  "row_number_1based": 1
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/0` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-001",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/0",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "selected-part-roster",
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-068"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-002

req~ssr-verification-methods~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/1/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-verification-methods~2",
  "component": "the supplier of each part",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "Evidence reported by its state with its revisions, configuration and limitations; evidence on each platform, target, backend and configuration that a claim names (for example heap windows on each claimed binding target, SR-279); results on test doubles kept apart from the real part. Not assessed.",
  "directive": "SR-279; SR-344; DR1-7; DR1-8",
  "cluster": "C01-governing"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/1/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/1",
  "row_number_1based": 2
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/1` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-002",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/1",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "selected-part-roster"
  ],
  "obligation_row_ids": [
    "RX03-OB-074"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-003

req~ssr-peer-fault-model~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/2/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-peer-fault-model~2",
  "component": "each part that a requirement of this area binds",
  "kind": "evidence_gap",
  "observation_case": "",
  "detail": "The model now names a peer's stall, exit, kill and restart; fault-injection evidence for these faults against each part's requirements, and the statement of the model in each part's safety manual, are not established.",
  "directive": "SR-281",
  "cluster": "C01-governing"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/2/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/2",
  "row_number_1based": 3
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/2` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-003",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/2",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "selected-part-roster"
  ],
  "obligation_row_ids": [
    "RX03-OB-069"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-004

req~ssr-element-boundary~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/3/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-element-boundary~2",
  "component": "the up-rust SEooC and its derive",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "No separate open item: this row duplicates C02-build-graph's follow-up against req~ssr-unsafe-inventory~4 (W2-C01b-F05). It stands on req~ssr-element-boundary~2, which since C01's round 2 (DD-C01-23) says only that the element's derive is a build input, and points to req~ssr-unsafe-inventory~4, whose clause \"Generated code\" states what holds for the code that the derive generates and who accounts for it. What this row records is held in C02's row: the generator-local inventory, proof and input constraints, and the element's accounting of the generated code that its build admits, are unassessed.",
  "directive": "XR-X021; XR-X028; XR-06",
  "cluster": "C01-governing",
  "as_recorded_by_cluster": {
    "detail": "The derive's generated code is element code: its inventory and proof coverage in the consuming build, for the exact derive and configuration (XR-06), is unassessed."
  },
  "amended_by": {
    "record": "integration/composition/B3b-C03/RECORD-AMENDMENTS.json",
    "source": "W2-C01b-F05"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/3/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/3",
  "row_number_1based": 4
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/3` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-004",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/3",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-072"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-005

req~ssr-item-reading~1; req~ssr-violation-classes~1

Historical disposition: **annotation_only**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/4/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-item-reading~1; req~ssr-violation-classes~1",
  "component": "each repository that covers SafetyUpRustCore, SafetyStreamer, SafetyBinding or SafetyWire items",
  "kind": "annotation_only",
  "observation_case": "",
  "detail": "Two new items with needs uman and four tags, and no gate tag (HD-06): each covering repository's trace record and safety manual have still to cover them.",
  "directive": "SR-106; SR-08",
  "cluster": "C01-governing"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/4/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/4",
  "row_number_1based": 5
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/4` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-005",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/4",
  "source_disposition": "annotation_only",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "selected-part-roster"
  ],
  "obligation_row_ids": [
    "RX03-OB-070",
    "RX03-OB-071"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-006

req~ssr-requirements-bind~3; req~ssr-contract-transition~2; req~ssr-peer-fault-model~2; req~ssr-element-boundary~2; req~ssr-element-configurations~2; req~ssr-verification-methods~2

Historical disposition: **annotation_only**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/5/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-requirements-bind~3; req~ssr-contract-transition~2; req~ssr-peer-fault-model~2; req~ssr-element-boundary~2; req~ssr-element-configurations~2; req~ssr-verification-methods~2",
  "component": "each repository that covers these items",
  "kind": "annotation_only",
  "observation_case": "",
  "detail": "Coverage tags and trace records that cite the earlier revisions have to move to the new ones; nothing is claimed for the new revisions until then.",
  "directive": "XR-02; HD-01",
  "cluster": "C01-governing"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/5/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/5",
  "row_number_1based": 6
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/5` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-006",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/5",
  "source_disposition": "annotation_only",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "selected-part-roster"
  ],
  "obligation_row_ids": [
    "RX03-OB-067",
    "RX03-OB-068",
    "RX03-OB-069",
    "RX03-OB-072",
    "RX03-OB-073",
    "RX03-OB-074"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-007

req~ssr-element-configurations~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/6/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-element-configurations~2",
  "component": "the supplier of the up-rust SEooC, for each supported configuration package",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Whether each supported configuration package states the sizes that the element's computation does not give (scheduler task and timer storage, allocator overhead, thread stacks), or names them as not established, is unassessed; no package was read in this correction.",
  "directive": "CCR-C04-19; XR-19; SR-07",
  "cluster": "C01-governing"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/6/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/6",
  "row_number_1based": 7
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/6` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-007",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/6",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-073"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-008

req~ssr-strict-states~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/7/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-states~1",
  "component": "the element's strict mode (part E7)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "New item: the default Off state, Report refusing nothing for a finding, the three severities and their Enforce refusals have not been assessed against the strict-mode implementation.",
  "directive": "SM-02, RT-02",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/7/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/7",
  "row_number_1based": 8
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/7` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-008",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/7",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-101"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-009

req~ssr-strict-lock~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/8/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-lock~3",
  "component": "the element's strict mode (part E7)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "The declared-graph check now runs in the Report and Enforce states with or without the strict-build setting, and a missing declared graph of an ASIL process is a forbidden finding; revision 2 required the refusal only under the setting. Not assessed.",
  "directive": "XR-X059, RT-02",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/8/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/8",
  "row_number_1based": 9
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/8` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-009",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/8",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-103"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-010

req~ssr-strict-report~4

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/9/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-report~4",
  "component": "the element's strict mode (part E7); the versioned item-status record",
  "kind": "evidence_gap",
  "observation_case": "O12",
  "detail": "The report reads each cited item's adoption status from the versioned status record of the declared specification revision; that record is designated, not shown populated (SR-10). \"Checks after a refusal or a failure\", the waiver entries and the severity column are unassessed.",
  "directive": "SR-07, SR-392, XR-D078, D05",
  "cluster": "C02-build-graph",
  "as_recorded_by_cluster": {
    "detail": "The report reads each cited item's adoption status from the versioned status record of the declared specification revision; that record is designated, not shown populated (SR-10). Findings after a refusal, the waiver entries and the severity column are unassessed."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W1-C02a-F03"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/9/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/9",
  "row_number_1based": 10
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/9` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-010",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/9",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-106"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-011

req~ssr-strict-ack-revision~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/10/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-ack-revision~3",
  "component": "the element's strict mode (part E7)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Per-route refusal within a deviation's stated scope (in place of a process-wide refusal) and the Report state's non-refusal are unassessed.",
  "directive": "D16, SM-11",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/10/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/10",
  "row_number_1based": 11
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/10` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-011",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/10",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-105"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-012

req~ssr-async-seam~4

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/11/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-async-seam~4",
  "component": "the asynchronous layer that implements the interface",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The SEooC README describes the executor-independent layer as planned and says that nothing claims it exists; the existing scheduler-specific integration is not shown to keep the contract. The contract's independence from the standard library and an allocator is unassessed.",
  "directive": "XR-07, XR-X009",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/11/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/11",
  "row_number_1based": 12
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/11` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-012",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/11",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-100"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-013

req~ssr-platform-contract~4

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/12/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-platform-contract~4",
  "component": "each platform backend of the element; the element",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "The platform contract test is required for each backend on the targets that its configuration names (QNX backends are source only); the new prefault and only-through-a-backend clauses have no assessed evidence.",
  "directive": "VM-20, SR-270",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/12/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/12",
  "row_number_1based": 13
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/12` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-013",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/12",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "rtos-platform"
  ],
  "remaining_scope_portfolios": [
    "other-platform-backends"
  ],
  "obligation_row_ids": [
    "RX03-OB-144"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-014

req~ssr-unsafe-inventory~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/13/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-unsafe-inventory~4",
  "component": "each generator of code for the element (the derive crate); the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The generator-local inventory, proof and input constraints, and the element's accounting of admitted generated code, are unassessed; the release gate on the element's inventory check is unassessed.",
  "directive": "XR-06, XR-X021, XR-X065",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/13/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/13",
  "row_number_1based": 14
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/13` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-014",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/13",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-141"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-015

req~ssr-release-lease-drop~6

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/14/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-release-lease-drop~6",
  "component": "the element; each binding, for its deferred return of a retained owner",
  "kind": "evidence_gap",
  "observation_case": "O6",
  "detail": "Eventual return of ownership does not show non-waiting, allocation-compliant release through dependencies; pending cleanup stays charged and isolated; a short measured wait is not non-waiting conformance. Isolation of pending cleanup and the call-completion versus slot-return order are unassessed.",
  "directive": "XR-16, RC-03 O6, XR-D226",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/14/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/14",
  "row_number_1based": 15
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/14` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-015",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/14",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-159"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-016

req~ssr-atomic-width~5

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/15/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-atomic-width~5",
  "component": "the element; each platform backend of the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The five properties now defined in the item, and the build failure on a target without 32-bit compare-and-swap, are unassessed against the targets of the supported configurations.",
  "directive": "VL-5, IR-19",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/15/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/15",
  "row_number_1based": 16
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/15` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-016",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/15",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "rtos-platform"
  ],
  "remaining_scope_portfolios": [
    "other-platform-backends"
  ],
  "obligation_row_ids": [
    "RX03-OB-148"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-017

req~ssr-crate-core-only-binary~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/16/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-crate-core-only-binary~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The restored non-bypass duty (no public way to build, enter runtime mode or carry traffic without the checked policy; strict mode lifts no check) has no assessed review.",
  "directive": "XR-X057, XR-X058",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/16/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/16",
  "row_number_1based": 17
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/16` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-017",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/16",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-109"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-018

req~ssr-crate-streamer-isolated~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/17/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-crate-streamer-isolated~4",
  "component": "the Streamer",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The dependency-closure gate and the build-setting isolation duty are new evidence requirements; unassessed.",
  "directive": "XR-D099",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/17/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/17",
  "row_number_1based": 18
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/17` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-018",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/17",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-110"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-019

req~ssr-unsafe-wire-untrusted~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/18/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-unsafe-wire-untrusted~2",
  "component": "the element (checked payload writer, part E1)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "New stated outcome (round 2, CCR-C11-01): the element refuses a frame for which the wire reports more bytes than it wrote, and sends or reads no byte that the wire did not write. Not assessed against the implementation. The item carries the gate tag SafetyGateUpRust: the covering repository's impl and utest tags name revision 1 and are to be moved to revision 2 only after that assessment (RX-03: no blind retagging).",
  "directive": "CCR-C11-01, SR-06, RT-02",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/18/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/18",
  "row_number_1based": 19
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/18` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-019",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/18",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-142"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-020

req~ssr-crate-core-only-binary~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/19/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-crate-core-only-binary~4",
  "component": "the element's strict mode (part E7)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "New clause (round 2, CCR-C01-10): strict mode's component needs no std and depends on no software component other than the safety crate; the gate (its manifest and a build on a sysroot without std) has no assessed evidence.",
  "directive": "CCR-C01-10, XR-X058",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/19/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/19",
  "row_number_1based": 20
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/19` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-020",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/19",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-109"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-021

req~ssr-async-seam~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/20/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-async-seam~4",
  "component": "the asynchronous layer that implements the interface; the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Property 6 now covers a transmit loan on a serialized route (round 2, CCR-C08-04): submitted or given back before its operation returns pending. Unassessed, in addition to the round-1 implementation gap of the layer itself.",
  "directive": "CCR-C08-04, XR-07",
  "cluster": "C02-build-graph"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/20/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/20",
  "row_number_1based": 21
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/20` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-021",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/20",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-100"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-022

req~ssr-bound-work~4

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/21/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-bound-work~4",
  "component": "the element",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "That each compare-exchange or load-linked/store-conditional retry follows another thread's progress, and never a peer's data, is not shown by a failed compare-exchange alone; the per-architecture mechanism argument moves from the assumptions page to QUAL-ELEM (C03-A removal of the 'Retry loops' mechanism sentences).",
  "directive": "D03",
  "cluster": "C03-assumptions",
  "as_recorded_by_cluster": {
    "detail": "That each compare-exchange or load-linked/store-conditional retry follows another thread's progress, and never a peer's data, is not shown by a failed compare-exchange alone; the per-architecture mechanism argument moves from the assumptions page to QUAL-ELEM (C03-A removal of the 'Retry loops' mechanism sentences)."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/21/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/21",
  "row_number_1based": 22
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/21` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-022",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/21",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-094"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-023

req~ssr-qos-deadline~6

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/22/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-deadline~6",
  "component": "the element; each binding (iceoryx2, LoLa)",
  "kind": "evidence_gap",
  "observation_case": "O4",
  "detail": "Whether every required arrival is recorded within the declared arrival latency L (zero on a push binding, at delivery) and whether a configuration's J covers the arrival path; without both, no finite detection bound may be claimed, and the assumption now budgets L and J separately.",
  "directive": "D04",
  "cluster": "C03-assumptions",
  "as_recorded_by_cluster": {
    "detail": "Whether every required arrival is recorded within the declared arrival latency L (zero on a push binding, at delivery) and whether a configuration's J covers the arrival path; without both, no finite detection bound may be claimed, and the assumption now budgets L and J separately."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/22/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/22",
  "row_number_1based": 23
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/22` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-023",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/22",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-153"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-024

req~ssr-mode-one-way~4, clause 'A fresh attempt before entry'; req~ssr-memory-sealed~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/23/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-mode-one-way~4, clause 'A fresh attempt before entry'; req~ssr-memory-sealed~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "A fresh, fully revalidated initialization in the same process after a refused one, admitted on the terms of a first one on startup memory that is not sealed, and every setup on a sealed startup memory refused; aou~seooc-supervisor-restart~6 now permits that attempt instead of requiring exit.",
  "directive": "D10",
  "cluster": "C03-assumptions",
  "as_recorded_by_cluster": {
    "detail": "A fresh, fully revalidated initialization in the same process after a refused one, admitted on the terms of a first one on startup memory that is not sealed, and every setup on a sealed startup memory refused; aou~seooc-supervisor-restart~6 now permits that attempt instead of requiring exit."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09 (adapted: the same prose form)"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/23/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/23",
  "row_number_1based": 24
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/23` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-024",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/23",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-168",
    "RX03-OB-175"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-025

req~ssr-e2e-detects~7 and the other requirements of e2e-wire.adoc, at their candidate revisions: req~ssr-e2e-not-required~2, req~ssr-e2e-wraps-any-wire~5, req~ssr-e2e-header-format~3, req~ssr-e2e-crate-isolated~2, req~ssr-e2e-resync~6, req~ssr-e2e-replay-window~2, req~ssr-e2e-rpc-echo~3, req~ssr-e2e-data-id~5, req~ssr-e2e-sender-state~3, req~ssr-e2e-receiver-state~3, req~ssr-e2e-refusal-counted~3, req~ssr-e2e-refusal-answered~2, req~ssr-e2e-figures~3, req~ssr-e2e-survives-hops~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/24/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-detects~7 and the other requirements of e2e-wire.adoc, at their candidate revisions: req~ssr-e2e-not-required~2, req~ssr-e2e-wraps-any-wire~5, req~ssr-e2e-header-format~3, req~ssr-e2e-crate-isolated~2, req~ssr-e2e-resync~6, req~ssr-e2e-replay-window~2, req~ssr-e2e-rpc-echo~3, req~ssr-e2e-data-id~5, req~ssr-e2e-sender-state~3, req~ssr-e2e-receiver-state~3, req~ssr-e2e-refusal-counted~3, req~ssr-e2e-refusal-answered~2, req~ssr-e2e-figures~3, req~ssr-e2e-survives-hops~2",
  "component": "the element's E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The wire is not delivered (its delivery status left the assumptions page); its contract stays normative and binds every route that selects it, and the assumption's E2E duties bind such a route whenever it is selected.",
  "directive": "SM-05; IA-8; RT-02",
  "cluster": "C03-assumptions",
  "as_recorded_by_cluster": {
    "detail": "The wire is not delivered (its delivery status left the assumptions page); its contract stays normative and binds every route that selects it, and the assumption's E2E duties bind such a route whenever it is selected."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/24/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/24",
  "row_number_1based": 25
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/24` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-025",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/24",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-115",
    "RX03-OB-116",
    "RX03-OB-117",
    "RX03-OB-118",
    "RX03-OB-119",
    "RX03-OB-120",
    "RX03-OB-121",
    "RX03-OB-122",
    "RX03-OB-123",
    "RX03-OB-124",
    "RX03-OB-125",
    "RX03-OB-126",
    "RX03-OB-127",
    "RX03-OB-128",
    "RX03-OB-129"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-026

req~ssr-waiver-record~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/25/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-waiver-record~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "Each active waiver recorded and reported at the transition; a waiver without its interference argument's kind and reference refused in the Enforce and Locked states and recorded as a finding, not refused, in the Report state; an unknown waiver id creates no waiver.",
  "directive": "D05; XR-D033",
  "cluster": "C03-assumptions"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/25/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/25",
  "row_number_1based": 26
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/25` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-026",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/25",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-179"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-027

req~ssr-config-one-path~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/26/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-config-one-path~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Every validation of a configuration runs in the element's setup, on the direct-construction path and the deployment-data path alike.",
  "directive": "XR-D100",
  "cluster": "C03-assumptions"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/26/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/26",
  "row_number_1based": 27
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/26` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-027",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/26",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-112"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-028

req~ssr-streamer-config-integrity~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/27/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-config-integrity~2",
  "component": "the Streamer",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Deployment data delivered with no integrity code refused before any part is applied, in every state, as data with a mismatching code is.",
  "directive": "D08",
  "cluster": "C03-assumptions",
  "as_recorded_by_cluster": {
    "detail": "Deployment data delivered with no integrity code refused before any part is applied, in every state, as data with a mismatching code is."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/27/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/27",
  "row_number_1based": 28
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/27` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-028",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/27",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-279"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-029

req~ssr-atomic-width~5

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/28/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-atomic-width~5",
  "component": "the element; each platform backend of the element, per supported configuration package",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "From research R1 (DD-C03-18): Rust 1.90.0 has no built-in target for RH850, TriCore or e200 VLE, so a package naming one needs its own target and lowering evidence (CAXI on RH850G3K); e200 reservation support is unverified per core; cross-core exclusive coherence of dual-core STM32H7 parts is not shown by a manual; the build failure on a target without 32-bit atomics with compare-and-swap is unchecked. Records: QUAL-ELEM.",
  "directive": "IR-19; SR-04",
  "cluster": "C03-assumptions",
  "as_recorded_by_cluster": {
    "detail": "From research R1 (DD-C03-18): Rust 1.90.0 has no built-in target for RH850, TriCore or e200 VLE, so a package naming one needs its own target and lowering evidence (CAXI on RH850G3K); e200 reservation support is unverified per core; cross-core exclusive coherence of dual-core STM32H7 parts is not shown by a manual; the build failure on a target without 32-bit atomics with compare-and-swap is unchecked. Records: QUAL-ELEM."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/28/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/28",
  "row_number_1based": 29
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/28` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-029",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/28",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "rtos-platform"
  ],
  "remaining_scope_portfolios": [
    "other-platform-backends"
  ],
  "obligation_row_ids": [
    "RX03-OB-148"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-030

req~ssr-qos-declared~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/29/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-declared~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "A detector route of the E2E wire with event_only() or no deadline refused in every state; a deadline together with event_only() refused in every state; an ASIL receive route that declares neither refused in the Enforce and Locked states and admitted in Off and Report.",
  "directive": "SM-05",
  "cluster": "C03-assumptions",
  "as_recorded_by_cluster": {
    "detail": "A detector route of the E2E wire with event_only() or no deadline refused in every state; a deadline together with event_only() refused in every state; an ASIL receive route that declares neither refused in the Enforce and Locked states and admitted in Off and Report."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/29/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/29",
  "row_number_1based": 30
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/29` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-030",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/29",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-150"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-031

req~ssr-bound-work~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/30/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-bound-work~4",
  "component": "the element (each lock-free retry loop)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Show for each compare-exchange loop that a repeat depends on another thread of the same process having made progress; a failed weak compare-exchange or store-conditional alone is no such progress. The mechanism described in the assumptions (repeat after a write in the reservation granule, or after an exception on load-linked/store-conditional cores) is assessed against this requirement in QUAL-ELEM.",
  "directive": "D03",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/30/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/30",
  "row_number_1based": 31
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/30` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-031",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/30",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-094"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-032

req~ssr-error-no-silent-loss~5

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/31/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-error-no-silent-loss~5",
  "component": "the element (the count); each pull binding, iceoryx2 and LoLa, under req~ssr-binding-arrival-reports~1 (the report)",
  "kind": "unchecked",
  "observation_case": "O4",
  "detail": "Each report of a failure to read the binding's own queue, and of a data connection that its middleware drops or fails to set up without an error, counted once in receive_failed and never as a missed arrival or a sample loss; the report itself is the binding's duty in C07's item.",
  "directive": "D14",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/31/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/31",
  "row_number_1based": 32
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/31` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-032",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/31",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-133"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-033

req~ssr-error-no-silent-loss~5

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/32/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-error-no-silent-loss~5",
  "component": "iceoryx2 binding at v0.9.3",
  "kind": "evidence_gap",
  "observation_case": "O2",
  "detail": "A retained corrupted sending connection skips a receiver while the send succeeds; the binding's named sending-side event and its degradation policy are stated by the binding item, req~ssr-binding-iceoryx2-degradation-handler~2 (CCR-C04-10), and the event's separation from receive_failed by this requirement's clause \"Sending-side connection faults\"; their evidence remains open.",
  "directive": "SR-221, XR-D179",
  "cluster": "C04-runtime-heap",
  "as_recorded_by_cluster": {
    "detail": "A retained corrupted sending connection skips a receiver while the send succeeds; the binding's named sending-side event, its degradation policy and its separation from receive_failed need the binding item (CCR-C04-10) and evidence."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-C04a-F05"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/32/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/32",
  "row_number_1based": 33
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/32` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-033",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/32",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-133"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-034

req~ssr-heap-none~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/33/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-none~4",
  "component": "the element; each wire used on a native route (req~ssr-wire-heap-claim-tested~4); each binding and its middleware (req~ssr-binding-no-heap~3)",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "No heap use in the work that a native operation causes, in the wire, the binding, shared control and release work; a heap-free wrapper does not establish heap-free middleware or peer-change handling.",
  "directive": "XR-15, XR-X015",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/33/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/33",
  "row_number_1based": 34
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/33` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-034",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/33",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "other-bindings",
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-135"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-035

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/34/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "each wire, binding and allocation provider; the element",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "New clauses: providers state their own policy and keep within it; refusal of a route with no stated class or policy; the startup report shows the policy of setup-only memory sources.",
  "directive": "XR-X016, XR-D070",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/34/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/34",
  "row_number_1based": 35
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/34` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-035",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/34",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "allocation-providers",
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-036

req~ssr-heap-guarantees~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/35/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-guarantees~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "A stated policy that does not exclude a violation leaves a guarantee established only on configuration-bound evidence; the absence of an observed failure establishes nothing.",
  "directive": "XR-D058, XR-D064",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/35/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/35",
  "row_number_1based": 36
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/35` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-036",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/35",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-138"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-037

req~ssr-heap-slot-types~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/36/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-slot-types~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "Outcome decided and recorded before the destruction and before any lock that it or its release needs; the route's report states where each destruction runs.",
  "directive": "XR-D059, XR-D094",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/36/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/36",
  "row_number_1based": 37
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/36` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-037",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/36",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-139"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-038

req~ssr-heap-lazy-forced~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/37/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-lazy-forced~1",
  "component": "the element on MCU targets that a supported configuration names",
  "kind": "evidence_gap",
  "observation_case": "",
  "detail": "The heap-window observers may not run on those targets; the window and its fault-injection control there are evidence work, not a weaker requirement.",
  "directive": "VL-10",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/37/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/37",
  "row_number_1based": 38
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/37` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-038",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/37",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "rtos-platform"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-136"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-039

req~ssr-mode-one-way~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/38/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-mode-one-way~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "A fresh initialization after a refused one in the same process: admitted as a first one, every check made again, nothing of the refused attempt reused; no post-entry re-admission, re-enabling, rebuilding or replacement.",
  "directive": "D10, SM-03",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/38/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/38",
  "row_number_1based": 39
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/38` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-039",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/38",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-168"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-040

req~ssr-mode-restart~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/39/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-mode-restart~3",
  "component": "each binding; the element",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "A binding that cannot recover what a predecessor left refuses setup by name (with CCR-C04-06); the element returns the refusal and admits no transport of that binding; the element initializes regardless of stale state.",
  "directive": "XR-X018",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/39/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/39",
  "row_number_1based": 40
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/39` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-040",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/39",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-170"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-041

req~ssr-memory-static~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/40/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-memory-static~3",
  "component": "the element; the Streamer",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Regions taken only through an interface whose contract hands them over exclusively for the life of the process, checked, never released in runtime mode; the roles map to the concrete crates in QUAL-ELEM.",
  "directive": "XR-X017",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/40/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/40",
  "row_number_1based": 41
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/40` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-041",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/40",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-174"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-042

req~ssr-mode-classic-excluded~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/41/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-mode-classic-excluded~3",
  "component": "the Streamer",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Each ASIL route built through the element's admission; a route configuration with a classic endpoint refused at setup by name.",
  "directive": "XR-X020",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/41/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/41",
  "row_number_1based": 42
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/41` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-042",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/41",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-172"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-043

req~ssr-memory-computable~5

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/42/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-memory-computable~5",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O7",
  "detail": "Computed capacity reported apart from the declared QoS depth and from middleware availability; figures that depend on unbounded inputs, a thread's stack included, reported as not established.",
  "directive": "IA-15, XR-D072",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/42/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/42",
  "row_number_1based": 43
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/42` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-043",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/42",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-176"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-044

req~ssr-refuse-no-panic~3

Historical disposition: **annotation_only**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/43/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-refuse-no-panic~3",
  "component": "the element",
  "kind": "annotation_only",
  "observation_case": "O12",
  "detail": "The lint check over element code replaces \"the workspace lints\" in the line; the repository configuration that implements it is mapped in the SDK trace record.",
  "directive": "VL-13, VM-12",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/43/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/43",
  "row_number_1based": 44
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/43` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-044",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/43",
  "source_disposition": "annotation_only",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-131"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-045

req~ssr-mode-no-shutdown~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/44/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-mode-no-shutdown~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "No degraded mode: after a refusal, a counted drop, a missed deadline or the loss of a peer on one route, no other route stopped, suspended or refused, and no route's rules changed (new clause, round 2).",
  "directive": "CCR-C01-04, SM-01",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/44/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/44",
  "row_number_1based": 45
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/44` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-045",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/44",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-171"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-046

req~ssr-error-route-report~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/45/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-error-route-report~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Each route's report states its form: native or serialized; lent or handed off; on a receive route of a lending binding, a read of the shared storage or the copy route's copy.",
  "directive": "CCR-C08-05",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/45/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/45",
  "row_number_1based": 46
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/45` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-046",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/45",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-134"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-047

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/46/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "Each route's report shows the declared policy of the route's binding where the binding allocates on the route, whatever the route requires (moved here from the frame-drop item, round 2).",
  "directive": "C07-CCR-03",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/46/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/46",
  "row_number_1based": 47
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/46` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-047",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/46",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-048

req~ssr-mode-entry~7

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/47/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-mode-entry~7",
  "component": "the Streamer; the element",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "The Streamer's process counts as in runtime mode only once enter_runtime_all has returned with every transport in runtime mode; a refused return seals startup memory and leaves the process not in runtime mode.",
  "directive": "CCR-C06-12, D09",
  "cluster": "C04-runtime-heap"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/47/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/47",
  "row_number_1based": 48
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/47` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-048",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/47",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-164"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-049

req~ssr-e2e-wraps-any-wire~5

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/48/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-wraps-any-wire~5",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/48/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/48",
  "row_number_1based": 49
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/48` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-049",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/48",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-116"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-050

req~ssr-e2e-header-format~3

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/49/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-header-format~3",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/49/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/49",
  "row_number_1based": 50
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/49` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-050",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/49",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-117"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-051

req~ssr-e2e-crate-isolated~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/50/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-crate-isolated~2",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/50/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/50",
  "row_number_1based": 51
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/50` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-051",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/50",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-118"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-052

req~ssr-e2e-detects~7

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/51/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-detects~7",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/51/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/51",
  "row_number_1based": 52
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/51` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-052",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/51",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-119"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-053

req~ssr-e2e-resync~6

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/52/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-resync~6",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/52/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/52",
  "row_number_1based": 53
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/52` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-053",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/52",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-120"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-054

req~ssr-e2e-replay-window~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/53/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-replay-window~2",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/53/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/53",
  "row_number_1based": 54
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/53` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-054",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/53",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-121"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-055

req~ssr-e2e-rpc-echo~3

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/54/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-rpc-echo~3",
  "component": "the E2E wire (part E6); the element, for one response wire per method and for refusing a response with no E2E header",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/54/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/54",
  "row_number_1based": 55
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/54` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-055",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/54",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-122"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-056

req~ssr-e2e-data-id~5

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/55/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-data-id~5",
  "component": "the E2E wire (part E6); the element, for the derivation function",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/55/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/55",
  "row_number_1based": 56
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/55` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-056",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/55",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-123"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-057

req~ssr-e2e-sender-state~3

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/56/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-sender-state~3",
  "component": "the E2E wire (part E6); the element's builders",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/56/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/56",
  "row_number_1based": 57
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/56` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-057",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/56",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-124"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-058

req~ssr-e2e-receiver-state~3

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/57/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-receiver-state~3",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/57/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/57",
  "row_number_1based": 58
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/57` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-058",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/57",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-125"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-059

req~ssr-e2e-figures~3

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/58/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-figures~3",
  "component": "the E2E wire (part E6)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/58/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/58",
  "row_number_1based": 59
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/58` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-059",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/58",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-128"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-060

req~ssr-e2e-refusal-answered~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/59/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-answered~2",
  "component": "the E2E wire (part E6); the element (RPC server and client roles)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The E2E wire is not implemented, integrated, compiled or qualified in the current delivery (external delivery record sdk-api-redesign-20261107-1/delivery-continuation/E2E-DEFERRED-20261115.json). The requirement stays normative ahead of implementation (SM-05); every case of its Verification line is unassessed.",
  "directive": "SM-05; RT-02; IA-8",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/59/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/59",
  "row_number_1based": 60
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/59` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-060",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/59",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-127"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-061

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/60/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element (part E1: end-to-end hooks and route counters)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The element-side naming and counting of end-to-end refusals, with a separate unprotected counter, is unassessed at any element revision; it has no E2E wire to exercise it in the current delivery.",
  "directive": "SM-05; XR-D089; SR-324",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/60/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/60",
  "row_number_1based": 61
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/60` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-061",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/60",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-062

req~ssr-e2e-not-required~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/61/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-not-required~2",
  "component": "the element (route admission; strict mode)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "Admission of routes without end-to-end protection in each of the four strict states, and the black-channel refusal in Enforce and Locked, are unassessed; a passing strict report shows only its named checks (O12).",
  "directive": "SM-05; SM-02; IR-11",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/61/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/61",
  "row_number_1based": 62
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/61` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-062",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/61",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-115"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-063

req~ssr-e2e-survives-hops~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/62/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-survives-hops~2",
  "component": "the Streamer",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The Streamer's E2E-hop checks are deferred with the E2E wire (delivery record E2E-DEFERRED-20261115.json: no Streamer E2E-hop gates in this delivery). Carriage of protected bytes rests on req~ssr-streamer-byte-identity, assessed separately.",
  "directive": "XR-X031; SM-05",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/62/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/62",
  "row_number_1based": 63
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/62` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-063",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/62",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-129"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-064

req~ssr-qos-declared~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/63/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-declared~3",
  "component": "the element (route admission)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Unassessed: the every-state refusal of a deadline together with event_only(); the every-state, every-level refusal of a detector route of the E2E wire that declares event_only() or no deadline (this replaces the earlier Enforce-only rule that admitted event_only() on the E2E wire); the Locked state named with Enforce. A design record at an older pin (design-plan-1/STAGE4-PLAN.md, section 6.5.4) planned no setup refusal in the E2E crate itself.",
  "directive": "SM-05; IR-2; DR1-11",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/63/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/63",
  "row_number_1based": 64
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/63` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-064",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/63",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-150"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-065

req~ssr-qos-deadline~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/64/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-deadline~6",
  "component": "the element (deadline monitor)",
  "kind": "unchecked",
  "observation_case": "O4",
  "detail": "Unassessed against the new revision: the first gap counted from the transport's entry into runtime mode (the earlier text counted gaps between arrivals only); the explicit resolution band; the transport's servicing bound; the report's D, L, r and bound. No element revision has been checked against them.",
  "directive": "D04; VL-4; SM-07",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/64/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/64",
  "row_number_1based": 65
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/64` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-065",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/64",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-153"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-066

req~ssr-qos-deadline~6

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/65/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-deadline~6",
  "component": "the iceoryx2 binding",
  "kind": "implementation_gap",
  "observation_case": "O4",
  "detail": "The iceoryx2 binding's own pages state that a notification that fails at the sender while the queue holds samples leaves an arrival unreported. Under req~ssr-qos-deadline~6 the no-false-miss premise then fails, and under req~ssr-binding-arrival-reports~1 every arrival is owed within L. Missing observation of every required arrival is a gap of the binding, not a relaxed premise (SM-07, O4). Exact assessment at iceoryx2 v0.9.3 is the binding owner's.",
  "directive": "SM-07; D11; D13",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/65/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/65",
  "row_number_1based": 66
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/65` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-066",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/65",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-153"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-067

req~ssr-qos-deadline~6

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/66/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-deadline~6",
  "component": "each pull binding (iceoryx2, LoLa)",
  "kind": "evidence_gap",
  "observation_case": "O4",
  "detail": "No pull binding's declared arrival latency L is shown to bound every report of every arrival (frame entering the queue, frame that the overflow policy discards) on each claimed target; the QoS contract's deadline checks on a subscriber and on a carrier ingress are the named evidence.",
  "directive": "D11; D12; XR-D223",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/66/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/66",
  "row_number_1based": 67
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/66` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-067",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/66",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-153"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-068

req~ssr-qos-deadline~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/67/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-deadline~6",
  "component": "the iceoryx2 binding",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Middleware event deadlines turned off in a configuration on the ASIL path: a design decision at an older pin built event services with deadlines disabled; not assessed at iceoryx2 v0.9.3.",
  "directive": "SR-03; SM-07",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/67/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/67",
  "row_number_1based": 68
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/67` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-068",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/67",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-153"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-069

req~ssr-qos-ttl~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/68/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-ttl~6",
  "component": "the element (TTL checks; carrier primitives)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Unassessed: a frame whose TTL is 0 never refused or counted as expired, also with the wall clock unreadable, at route entry, at take and before a carrier forwards it (the earlier text's letter refused such a frame about one millisecond after its creation); the refusal of an RPC request whose TTL is 0 at a client and at a server under its own cause. A public up-rust validator rejects a zero request TTL at the source level (research R2, bounded inspection at a fixed commit); no element revision of this SDK was assessed.",
  "directive": "SM-04; IR-13",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/68/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/68",
  "row_number_1based": 69
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/68` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-069",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/68",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-154"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-070

req~ssr-qos-ttl~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/69/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-ttl~6",
  "component": "each binding whose middleware carries a message lifetime",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "No binding's statement of its TTL-to-lifetime mapping (zero, absent and positive values, both directions) has been assessed.",
  "directive": "SM-04; XR-D261",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/69/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/69",
  "row_number_1based": 70
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/69` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-070",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/69",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-154"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-071

req~ssr-qos-no-downgrade~5

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/70/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-no-downgrade~5",
  "component": "the element (route admission; strict mode)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "Unassessed: in the Enforce and Locked states, refusal of a route with a declared reaction budget for which no finite detection bound is established.",
  "directive": "IR-13; D07",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/70/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/70",
  "row_number_1based": 71
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/70` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-071",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/70",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-151"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-072

req~ssr-qos-overflow~3

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/71/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-overflow~3",
  "component": "the iceoryx2 binding",
  "kind": "evidence_gap",
  "observation_case": "O3",
  "detail": "A drop-oldest eviction inside the middleware is not observable to the binding: the route's report must disclose the limitation and claim no complete count; a zero local counter proves no absence of middleware losses (O3). The disclosure is unassessed.",
  "directive": "IA-16; XR-R038",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/71/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/71",
  "row_number_1based": 72
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/71` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-072",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/71",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-152"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-073

req~ssr-qos-consistent-declarations~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/72/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-consistent-declarations~2",
  "component": "the element (route admission)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "A design record at an older pin (design-plan-1/STAGE4-PLAN.md, item 5 of its opening findings) observed that the peer-mismatch refusal existed but was never raised. Unassessed at the current element; the new revision also forbids refusing declarations that differ only in depth, deadline or required priority.",
  "directive": "SM-04",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/72/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/72",
  "row_number_1based": 73
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/72` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-073",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/72",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-156"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-074

req~ssr-qos-invalid-refused~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/73/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-invalid-refused~2",
  "component": "the element (route admission)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The same older design record observed that a zero wait timeout and history on an RPC route were accepted. Unassessed at the current element.",
  "directive": "XR-X030",
  "cluster": "C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/73/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/73",
  "row_number_1based": 74
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/73` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-074",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/73",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-155"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-075

req~ssr-streamer-route-gate~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/74/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-route-gate~3",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: no in-process return to enabled; further causes recorded with the gate state unchanged; each cause shown in the route report. Revision 2 permitted an operator re-enable, so an implementation of revision 2 may offer one.",
  "directive": "SM-03",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/74/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/74",
  "row_number_1based": 75
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/74` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-075",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/74",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-283"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-076

req~ssr-streamer-operator-control~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/75/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-operator-control~3",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: the interface offers no re-enable or trip-clearing operation; strict refusal only in Enforce and Locked; Report-state finding in the startup report.",
  "directive": "SM-03; SR-132",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/75/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/75",
  "row_number_1based": 76
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/75` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-076",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/75",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-284"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-077

req~ssr-streamer-monitors~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/76/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-monitors~6",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: latch against operator and cleared condition; refusal of a peer with no declared restart time in Enforce and Locked; declared trip conditions. Round 2: each monitor's trip condition stated for integrators (needs uman).",
  "directive": "SM-03; IA-19",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/76/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/76",
  "row_number_1based": 77
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/76` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-077",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/76",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-285"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-078

req~ssr-streamer-dispatch-obligations~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/77/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-dispatch-obligations~3",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "O3",
  "detail": "Assess: four admission-refusal causes distinct from expiry and overflow; no resend or forwarding through a gate that is not enabled; parked frames of such a route counted as dropped. Also: a positive TTL that cannot be judged because the wall clock cannot be read is refused by the TTL check and counted, not forwarded; a TTL of 0 is not refused then.",
  "directive": "SR-12",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/77/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/77",
  "row_number_1based": 78
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/77` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-078",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/77",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-288"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-079

req~ssr-streamer-rpc-error-response~5

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/78/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-rpc-error-response~5",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "O5",
  "detail": "Assess: PermissionDenied for an encoding-filter refusal; Unavailable for a closed gate; reserved capacity, no block, no retry, sent/not-sent counts.",
  "directive": "SR-12; SR-14",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/78/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/78",
  "row_number_1based": 79
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/78` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-079",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/78",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-289"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-080

dsn~dispatcher-at-least-once~1; dsn~dispatcher-dead-letter-topic~1; dsn~dispatcher-rpc-error-response~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/79/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "dsn~dispatcher-at-least-once~1; dsn~dispatcher-dead-letter-topic~1; dsn~dispatcher-rpc-error-response~1",
  "component": "the Streamer (crate and binary)",
  "kind": "implementation_gap",
  "observation_case": "",
  "detail": "Deliberate scoped difference (req~ssr-streamer-dispatch-obligations~3, req~ssr-streamer-rpc-error-response~5): the Streamer cannot also meet the unamended chapter rules, so no conformance claim to them is possible for it until MP-C06-01 to MP-C06-03 are adopted (req~ssr-contract-transition~2, \"Where an item meets a chapter's rule\", second case).",
  "directive": "SR-12; XR-21",
  "cluster": "C06-l2-streamer",
  "as_recorded_by_cluster": {
    "detail": "Deliberate scoped difference (req~ssr-streamer-dispatch-obligations~3, req~ssr-streamer-rpc-error-response~5): the Streamer cannot also meet the unamended chapter rules, so no conformance claim to them is possible for it until MP-C06-01 to MP-C06-03 are adopted (req~ssr-contract-transition~2, \"Where an item meets a chapter's rule\", second case)."
  },
  "amended_by": {
    "record": "integration/composition/HOST-B3/RECORD-AMENDMENTS.json",
    "source": "B3b-C06-NR4"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/79/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/79",
  "row_number_1based": 80
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/79` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-080",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/79",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-364",
    "RX03-OB-365",
    "RX03-OB-366"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-081

req~ssr-streamer-qos-per-leg~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/80/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-qos-per-leg~3",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: encoding-policy denial as its own cause; exactly-once counting with the element's count as the one count; TTL 0 not refused; setup report of each route's filter.",
  "directive": "SR-13; SR-15; VL-19",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/80/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/80",
  "row_number_1based": 81
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/80` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-081",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/80",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-276"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-082

req~ssr-streamer-startup-report~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/81/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-startup-report~4",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "Assess: digest over the applied data; strict findings; reaction-bound statement. A clean report is evidence of its own checks only. Also: the report only once every transport is in runtime mode; a failed enter_runtime_all reported as a refused startup. Round 2: a refused startup's report names its cause (missing integrity code or mismatch, refused field, route and limit, failed build or transition). After the Opus pass: workers listed with their settings; each refusal cause named.",
  "directive": "D09; SR-15; CCR-C03-08",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/81/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/81",
  "row_number_1based": 82
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/81` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-082",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/81",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-278"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-083

req~ssr-streamer-config-integrity~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/82/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-config-integrity~2",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: data with no code refused; refusal in every strict state, Off included.",
  "directive": "D08",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/82/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/82",
  "row_number_1based": 83
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/82` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-083",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/82",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-279"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-084

req~ssr-streamer-build-order~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/83/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-build-order~2",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: a refused route or failed build refuses the whole startup, with no runtime entry and no route activated. After the Opus pass: refused interface, monitor, subscription source and uncreatable worker each refusing the startup.",
  "directive": "IA-17; IR-8",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/83/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/83",
  "row_number_1based": 84
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/83` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-084",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/83",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-277"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-085

req~ssr-streamer-workers~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/84/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-workers~2",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: binary creates and assigns workers; the Streamer's loop never returns in runtime mode. After the Opus pass: a worker that the binary cannot create reported, and the startup refused.",
  "directive": "XR-X046",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/84/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/84",
  "row_number_1based": 85
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/84` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-085",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/84",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-282"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-086

req~ssr-streamer-containment~2; req~ssr-streamer-no-ingress-block~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/85/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-containment~2; req~ssr-streamer-no-ingress-block~2",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: each route held to its declared budget, the faulty one included; no egress blocks an ingress or worker beyond those budgets.",
  "directive": "XR-X048",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/85/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/85",
  "row_number_1based": 86
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/85` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-086",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/85",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-274",
    "RX03-OB-275"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-087

req~ssr-streamer-byte-identity~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/86/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-byte-identity~3",
  "component": "the element (up-rust-core runtime path and carrier primitives)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Now binds the element too: test the cases on carrier routes of the carrier primitives without the Streamer (no identifier refusal, one copy into a lending egress, absence kept).",
  "directive": "XR-X053",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/86/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/86",
  "row_number_1based": 87
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/86` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-087",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/86",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-273"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-088

req~ssr-carrier-lending-service~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/87/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-carrier-lending-service~1",
  "component": "the element (up-rust-core runtime path and carrier primitives)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "New item from a rationale unit. A retained source copy of the carrier primitives shows a physical-address check of this kind; that is no assessment.",
  "directive": "Supplement 1 U033",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/87/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/87",
  "row_number_1based": 88
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/87` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-088",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/87",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-272"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-089

req~ssr-peer-rpc-no-server~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/88/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-peer-rpc-no-server~2",
  "component": "the element (up-rust-core runtime path and carrier primitives); each binding that declares an exact receiver count",
  "kind": "evidence_gap",
  "observation_case": "O5",
  "detail": "The fail-at-once path needs a binding report of the exact servers that received a request at its handover; notification listener counts (for example the iceoryx2 Ok(n)) and discovery are not that report. Without it the call ends at its TTL.",
  "directive": "O5; IR-20",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/88/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/88",
  "row_number_1based": 89
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/88` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-089",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/88",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-262"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-090

req~ssr-peer-rpc-ends~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/89/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-peer-rpc-ends~4",
  "component": "the element (up-rust-core runtime path and carrier primitives)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: completion only by a response accepted before the TTL; slot return only after pending releases. The refusal of an RPC request whose TTL is 0 is assessed against req~ssr-qos-ttl (cluster C05).",
  "directive": "SM-04; XR-D226",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/89/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/89",
  "row_number_1based": 90
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/89` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-090",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/89",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-258"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-091

req~ssr-peer-rpc-correlation~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/90/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-peer-rpc-correlation~4",
  "component": "the element (up-rust-core runtime path and carrier primitives)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: in the claim-first mechanism, a mismatched response completes and records nothing and the claimed slot is given back.",
  "directive": "XR-D227",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/90/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/90",
  "row_number_1based": 91
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/90` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-091",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/90",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-259"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-092

req~ssr-ids-unique~5

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/91/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-ids-unique~5",
  "component": "the element (up-rust-core runtime path and carrier primitives)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: heap-free ID generation on serialized routes that declare allocation; a persistent construction and its stated conditions in any package that offers never-repeating IDs.",
  "directive": "XR-D224; XR-X050",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/91/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/91",
  "row_number_1based": 92
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/91` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-092",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/91",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-263"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-093

req~ssr-peer-rpc-undecodable-answered~7

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/92/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-peer-rpc-undecodable-answered~7",
  "component": "the element (up-rust-core runtime path and carrier primitives)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Assess: a request that is corrupt and names an undeclared data ID is not answered; the InvalidArgument answer to an undeclared data ID is counted and sent under the route's overflow policy; the client-side call ends at its TTL only where no accepted response completes it. Round 2: on a route that does not select the E2E wire, a request whose TTL is 0, one with no payload where its operation requires a value and one of an encoding that no wire declares answered InvalidArgument, once, and counted once; on a route that selects the E2E wire, no request refused before the E2E wire reads it answered. After R2B: the checks of a request in the stated order, each refused request counted once under the first refusing check's cause, also where a result is returned, and an unsent answer counted as an event of its own. After R2C and the Opus pass: answers by the refusing check; no answer to expired or unjudgeable requests; no-header requests counted as unprotected (C05's rule); a caller given a refusal's result gets no means to answer.",
  "directive": "D01; VL-23; CCR-C05-19; R2A-002",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/92/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/92",
  "row_number_1based": 93
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/92` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-093",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/92",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-261"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-094

req~ssr-rpc-check-at-arrival~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/93/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-rpc-check-at-arrival~3",
  "component": "the element (up-rust-core runtime path and carrier primitives)",
  "kind": "unchecked",
  "observation_case": "O5",
  "detail": "Assess: a response completes a call only after the tie to the call and the wire check; a refusal answer is checked like any response before it has an effect. Round 2: the pre-wire encoding admission applied before any wire, its refusal counted once and the call kept open. After the Opus pass: TTL check after the tie; failed responses completing their call on a route without the E2E wire; the advisory kind named at the TTL.",
  "directive": "D02; D06; D15",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/93/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/93",
  "row_number_1based": 94
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/93` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-094",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/93",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-260"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-095

aou~seooc-ids-never-repeat~1; aou~seooc-streamer-reaction-bound~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/94/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "aou~seooc-ids-never-repeat~1; aou~seooc-streamer-reaction-bound~1",
  "component": "integrator documentation (the safety manuals of the SDK and of the Streamer)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "New assumptions of use: each safety manual has to state them; no manual was read for this correction.",
  "directive": "XR-X050; XR-D237",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/94/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/94",
  "row_number_1based": 95
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/94` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-095",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/94",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "deployment-records"
  ],
  "obligation_row_ids": [
    "RX03-OB-264",
    "RX03-OB-292"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-096

req~ssr-streamer-no-panic~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/95/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-no-panic~1",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "New item (round 2). Assess: the panic-link check over the Streamer's runtime paths with opaque inputs; the lint check over the Streamer's and the Streamer binary's own code on runtime paths; a review of the Streamer binary's own code that is reachable in runtime mode; fault injection without a panic or an abort. A build that passes is evidence of the named checks only.",
  "directive": "VL-6",
  "cluster": "C06-l2-streamer",
  "as_recorded_by_cluster": {
    "detail": "New item (round 2). Assess: the panic-link check and the lint check over the Streamer's own runtime paths with opaque inputs; fault injection without a panic or an abort. A build that passes is evidence of the named checks only."
  },
  "amended_by": {
    "record": "integration/composition/B3b-C06/RECORD-AMENDMENTS.json",
    "source": "W2-C06a-F03"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/95/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/95",
  "row_number_1based": 96
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/95` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-096",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/95",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-269"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-097

req~ssr-streamer-config-validated~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/96/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-config-validated~1",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "New item (round 2). Assess: complete validation of the Streamer's deployment data, including the worker settings that the Streamer binary applies (ranges, dependencies), published ranges, refusal by field, nothing coerced, defaulted or skipped; the binary's startup path applies no value before validation and none of refused data. After the Opus pass: strict-mode findings kept out of validation refusals.",
  "directive": "CCR-C03-08; HD-02",
  "cluster": "C06-l2-streamer"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/96/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/96",
  "row_number_1based": 97
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/96` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-097",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/96",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-280"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-098

req~ssr-binding-arrival-reports~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/97/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-arrival-reports~1",
  "component": "the iceoryx2 binding (built against iceoryx2 v0.9.3)",
  "kind": "evidence_gap",
  "observation_case": "O4",
  "detail": "Not established that every arrival of a pull route with a deadline, each queue entry and each overflow discard, is reported within a declared L; its reports come from its receive thread on drained notifications or a queue turning non-empty. A wake hint or a notification-failure count does not establish every arrival (RC-03 O4).",
  "directive": "D11; D12; D13",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/97/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/97",
  "row_number_1based": 98
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/97` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-098",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/97",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-187"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-099

req~ssr-binding-arrival-reports~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/98/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-arrival-reports~1",
  "component": "the iceoryx2 binding (built against iceoryx2 v0.9.3)",
  "kind": "evidence_gap",
  "observation_case": "O1",
  "detail": "'Each event as what it is': at v0.9.3 a notifier's Ok(n) is a listener count and per-listener failures may leave Ok(n); Err occurs after fan-out in some cases. The binding must report and count notification-call failures, listener-attempt failures, connection faults, arrivals and losses apart; its mapping is to be assessed.",
  "directive": "SM-07; D17",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/98/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/98",
  "row_number_1based": 99
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/98` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-099",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/98",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-187"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-100

req~ssr-binding-arrival-reports~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/99/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-arrival-reports~1",
  "component": "the iceoryx2 binding (built against iceoryx2 v0.9.3)",
  "kind": "evidence_gap",
  "observation_case": "O2",
  "detail": "A retained corrupted sending connection can skip a receiver while other recipients succeed: the binding's connection-fault report and its separation from loss and arrival are to be assessed.",
  "directive": "SM-07",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/99/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/99",
  "row_number_1based": 100
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/99` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-100",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/99",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-187"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-101

req~ssr-binding-arrival-reports~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/100/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-arrival-reports~1",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": "O4",
  "detail": "Declared arrival-report latency, arrival reports at the queue or overflow event, failed-read reports and event separation are unassessed.",
  "directive": "D11; D12; D13; D14",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/100/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/100",
  "row_number_1based": 101
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/100` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-101",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/100",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-187"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-102

req~ssr-binding-arrival-reports~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/101/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-arrival-reports~1",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "'Failed reads': each pull binding's report of a failed read of its own queue and of a data connection dropped or not set up without an error is unassessed.",
  "directive": "D14",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/101/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/101",
  "row_number_1based": 102
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/101` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-102",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/101",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-187"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-103

req~ssr-binding-response-delivery~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/102/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-response-delivery~1",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "One-at-a-time delivery on each response route served by push is unassessed for every binding that serves one.",
  "directive": "D15",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/102/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/102",
  "row_number_1based": 103
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/102` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-103",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/102",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-188"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-104

req~ssr-binding-no-heap~3

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/103/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-no-heap~3",
  "component": "each binding",
  "kind": "evidence_gap",
  "observation_case": "O10",
  "detail": "Heap windows are required on each target and configuration that the binding's qualification record claims; evidence measured on another target is no evidence for a claimed one (RF23). A heap-free wrapper does not establish heap-free middleware or peer-change handling.",
  "directive": "SR-279; XR-15",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/103/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/103",
  "row_number_1based": 104
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/103` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-104",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/103",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-181"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-105

req~ssr-binding-frame-drop~10

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/104/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-frame-drop~10",
  "component": "each binding",
  "kind": "evidence_gap",
  "observation_case": "O6",
  "detail": "On a native route, a release that the deferred return performs must free no heap, middleware work included, and never free the backing. Eventual return of ownership does not show nonblocking, allocation-free release through dependencies.",
  "directive": "XR-D165; XR-16",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/104/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/104",
  "row_number_1based": 105
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/104` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-105",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/104",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-183"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-106

req~ssr-binding-no-panic~9

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/105/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-no-panic~9",
  "component": "the iceoryx2 binding (built against iceoryx2 v0.9.3)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "The account must now be stated in the binding's qualification record, with the accepted instance (history-queue push counter) disclosed as a deviation and shown unreachable by peer input, death, restart or a faulty shared-memory write; the record's content is unassessed.",
  "directive": "XR-14; XR-D160",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/105/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/105",
  "row_number_1based": 106
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/105` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-106",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/105",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-184"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-107

req~ssr-binding-no-panic~9

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/106/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-no-panic~9",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": "O11",
  "detail": "The record must state that the binding has no accepted panic path; foreign termination paths (terminate, abort, an uncaught exception) reachable by a peer remain known failures; catching exceptions does not establish their absence.",
  "directive": "XR-14; XR-13",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/106/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/106",
  "row_number_1based": 107
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/106` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-107",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/106",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-184"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-108

req~ssr-binding-readme~3

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/107/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "each binding",
  "kind": "implementation_gap",
  "observation_case": "",
  "detail": "Each qualification record (QUAL-IOX, QUAL-LOLA) must name its claimed targets and configurations, class every entry as a limit, a deviation (with identifier, revision, scope, bounds, evidence, lapse date) or a known failure, and say for each whether the cause is the binding's code or its middleware with the exact source. No populated record meeting this is known.",
  "directive": "XR-13; SM-11; DR1-14",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/107/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/107",
  "row_number_1based": 108
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/107` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-108",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/107",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-109

req~ssr-binding-readme~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/108/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O9",
  "detail": "For a middleware with shared memory, the record must disclose the four shared-memory points; a label, matching layout, any-bits payload or private copy does not show safe middleware offsets or isolation.",
  "directive": "XR-17",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/108/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/108",
  "row_number_1based": 109
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/108` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-109",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/108",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-110

req~ssr-binding-qos-capability~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/109/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-qos-capability~2",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": "O7",
  "detail": "Each declared count (TX loans, retained RX samples, middleware-kept samples) must state its resource, unit, scope and whether it is an upper limit or guaranteed availability; setup storage and admission are to be checked against those meanings.",
  "directive": "SM-08; XR-13",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/109/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/109",
  "row_number_1based": 110
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/109` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-110",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/109",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-186"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-111

req~ssr-binding-unsafe-inventory~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/110/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-unsafe-inventory~2",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Each binding needs a reviewed inventory, empty where it has no unsafe code, and build and release verification that refuses a new, changed or vanished site.",
  "directive": "XR-06; SR-312",
  "cluster": "C07-bindings",
  "as_recorded_by_cluster": {
    "detail": "Each binding with unsafe code of its own needs a reviewed inventory and build and release verification that refuses a new, changed or vanished site; now independent of shipping status."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W1-C07a-F11"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/110/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/110",
  "row_number_1based": 111
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/110` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-111",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/110",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-194"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-112

req~ssr-binding-predecessor-cleanup~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/111/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-predecessor-cleanup~2",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "Refusal of setup by name when a needed recovery or removal of a dead predecessor's resources fails is unassessed; no cleanup after a dead peer once the transport is in runtime mode.",
  "directive": "XR-16; XR-X037",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/111/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/111",
  "row_number_1based": 112
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/111` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-112",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/111",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-191"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-113

aou~seooc-qualified-bindings-and-wires~9

Historical disposition: **annotation_only**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/112/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "aou~seooc-qualified-bindings-and-wires~9",
  "component": "the SDK safety manual and its quotation checker",
  "kind": "annotation_only",
  "observation_case": "O12",
  "detail": "The revised assumptions (aou~seooc-peer-access~7, aou~seooc-integrity-levels~8, aou~seooc-qualified-bindings-and-wires~9) need the safety-manual registry of quoted assumptions updated; tooling work, no spec-adoption wait (SR-277).",
  "directive": "SR-277",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/112/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/112",
  "row_number_1based": 113
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/112` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-113",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/112",
  "source_disposition": "annotation_only",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-199"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-114

req~ssr-binding-receiver-count~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/113/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-receiver-count~1",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O5",
  "detail": "Whether a binding can establish, at each request handover, the exact number of servers that received the request is unassessed. A count of notification listeners or a discovery result does not establish it; a binding that cannot establish it must not declare an exact receiver count, and its calls then end at their TTL.",
  "directive": "O5; XR-X030; CCR-C06-04",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/113/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/113",
  "row_number_1based": 114
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/113` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-114",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/113",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-189"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-115

req~ssr-binding-lazy-forced~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/114/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lazy-forced~2",
  "component": "each binding",
  "kind": "evidence_gap",
  "observation_case": "O10",
  "detail": "The rule now names the middleware's first locks and lazy primitives that a runtime path of the binding can reach: for each binding, a source-level review at the middleware revision it is built against, and first-traffic heap windows on each claimed target, are needed; a lazily initialized middleware lock or primitive on first traffic is a departure to be recorded.",
  "directive": "XR-15; CCR-C10-05",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/114/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/114",
  "row_number_1based": 115
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/114` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-115",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/114",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-182"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-116

req~ssr-binding-predecessor-cleanup~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/115/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-predecessor-cleanup~2",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "No route prepared over what could not be recovered, and no refusal of setup merely for the presence of a predecessor's leftovers, are unassessed.",
  "directive": "XR-X018; XR-X037; CCR-C04-06",
  "cluster": "C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/115/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/115",
  "row_number_1based": 116
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/115` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-116",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/115",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-191"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-117

req~ssr-integrity-zero-copy~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/116/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-zero-copy~6",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O9",
  "detail": "Copy route: one source-access mechanism per target and mapping, valid against arbitrary peer writes (R1: atomic loads on read-only memory only for small relaxed loads on listed targets; reader-side atomics against a plain-writing peer are a data race; cross-process mapping needs a platform contract); header and metadata acquired separately; payload extent only, within the binding-given length and route bound; no value before validation; validation heap-free, decoding included; setup refusal of a copy route on a wire that allocates when decoding.",
  "directive": "XR-17; XR-D125",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/116/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/116",
  "row_number_1based": 117
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/116` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-117",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/116",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-217"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-118

req~ssr-integrity-zero-copy~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/117/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-zero-copy~6",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O9",
  "detail": "Decoding from shared memory only with a known writer at or above the reader and the tenure and synchronization conditions; otherwise the copy route.",
  "directive": "DR2-6",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/117/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/117",
  "row_number_1based": 118
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/117` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-118",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/117",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-217"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-119

req~ssr-integrity-zero-copy~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/118/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-zero-copy~6",
  "component": "the element; each lending binding (iceoryx2, LoLa)",
  "kind": "unchecked",
  "observation_case": "O9",
  "detail": "Shared-state direction stated at setup by each lending binding; the element treats an unstated direction as two-way and refuses a port of another level on every port kind.",
  "directive": "XR-17",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/118/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/118",
  "row_number_1based": 119
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/118` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-119",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/118",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-217"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-120

req~ssr-integrity-zero-copy~6

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/119/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-zero-copy~6",
  "component": "the element",
  "kind": "evidence_gap",
  "observation_case": "O9",
  "detail": "Newly named Miri negative control (an atomic read racing a plain write) and review of the target and mapping semantics; existing evidence predates them.",
  "directive": "SM-10",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/119/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/119",
  "row_number_1based": 120
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/119` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-120",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/119",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-217"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-121

req~ssr-integrity-declared~5

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/120/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-declared~5",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O9",
  "detail": "A writer level reported as unknown, or not reported, on a lending binding treated as below every level; refusal names the route and both levels.",
  "directive": "DR2-19",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/120/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/120",
  "row_number_1based": 121
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/120` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-121",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/120",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-216"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-122

req~ssr-integrity-any-bits~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/121/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-any-bits~2",
  "component": "the derive (element derive macros)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Build-time refusal of invalid-bit-pattern fields and generated checked getters, now allocated to the derive; generator inventory and proof in its records (XR-06).",
  "directive": "XR-X028",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/121/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/121",
  "row_number_1based": 122
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/121` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-122",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/121",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-218"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-123

req~ssr-integrity-carrier-crossing~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/122/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-carrier-crossing~3",
  "component": "the element (carrier primitives)",
  "kind": "unchecked",
  "observation_case": "O9",
  "detail": "Carrier trigger uses the ingress writer's reported level (or unknown) against the egress level; header and metadata acquired separately; payload placed within the frame extent by checked arithmetic and copied once into the payload-only egress loan; nothing sent before re-admission; acquisition, copy, validation and re-admission heap-free in own and invoked work.",
  "directive": "XR-17; XR-12",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/122/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/122",
  "row_number_1based": 123
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/122` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-123",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/122",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-220"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-124

req~ssr-peer-failure-normal~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/123/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-peer-failure-normal~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "Survivor operations, accounting and health reports never wait for a failure release; only an owner whose release needs the same stalled dependency waits; nothing the failure did not end is released or revoked, on every route.",
  "directive": "XR-D178",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/123/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/123",
  "row_number_1based": 124
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/123` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-124",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/123",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-241"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-125

req~ssr-native-abi-agreed~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/124/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-native-abi-agreed~2",
  "component": "each binding that carries native frames between processes (iceoryx2, LoLa)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Records the element fingerprint per route, reports found and missing fingerprints, never reports one it did not find.",
  "directive": "XR-X035",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/124/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/124",
  "row_number_1based": 125
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/124` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-125",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/124",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-242"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-126

req~ssr-native-abi-agreed~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/125/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-native-abi-agreed~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Setup refusal of a missing fingerprint (previously a native peer refusal of unclear actor) naming route and missing fingerprint; no converted or reinterpreted route.",
  "directive": "XR-X035",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/125/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/125",
  "row_number_1based": 126
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/125` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-126",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/125",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-242"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-127

req~ssr-payload-written-once~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/126/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-payload-written-once~3",
  "component": "each handoff binding",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "One copy per direction at the hand-off and no other copy in binding code.",
  "directive": "XR-D197",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/126/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/126",
  "row_number_1based": 127
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/126` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-127",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/126",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-238"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-128

req~ssr-payload-written-once~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/127/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-payload-written-once~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Serialized storage before it is sent: one owner, consumed by a send with no later write through it, drop gives it back once, a forgotten owner keeps it charged, and no pending operation holds a transmit loan.",
  "directive": "Supplement 1 (U090); XR-16",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/127/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/127",
  "row_number_1based": 128
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/127` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-128",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/127",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-238"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-129

req~ssr-payload-route-form~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/128/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-payload-route-form~2",
  "component": "each binding; the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Smallest payload in the binding capability, compared with the route's (for sending the wire's stated smallest encoding or zero; for receiving zero); refusal reasons limited to declared ones; no binding refusal by wire or encoding name; no runtime change of lending; operational setup failures returned as named initialization errors.",
  "directive": "SR-118; XR-D198",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/128/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/128",
  "row_number_1based": 129
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/128` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-129",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/128",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-239"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-130

req~ssr-wire-encoding-checked~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/129/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-encoding-checked~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Result missing_payload for a typed operation that requires a value (returned, or counted once under that cause; the previous revision left it undecided); absence reported as an API contract states.",
  "directive": "XR-22",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/129/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/129",
  "row_number_1based": 130
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/129` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-130",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/129",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-240"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-131

req~ssr-wire-encoding-checked~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/130/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-encoding-checked~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Encoding comparison before any wire, the end-to-end protection wire included; typed sending gives the identifier that the wire declares.",
  "directive": "XR-X029; XR-D200",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/130/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/130",
  "row_number_1based": 131
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/130` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-131",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/130",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-240"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-132

req~ssr-wire-encoding-checked~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/131/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-encoding-checked~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "On an RPC client's response route, or an RPC server's request route, that selects the E2E wire, a message that carries no payload of the E2E wire's encoding is refused before any wire and counted once as unprotected (req~ssr-e2e-refusal-counted~3). This covers a message with no payload, a payload of another encoding (the wrapped wire's included), and a present payload of length zero under an identifier other than the E2E wire's. There is no mismatch or missing_payload result or count, no absence is reported, and no wire is given the payload, the empty input included. Not checked against any implementation.",
  "directive": "XR-X029; CCR-C05-18",
  "cluster": "C08-integrity-peers-paths",
  "as_recorded_by_cluster": {
    "detail": "On an RPC client response route that selects the E2E wire, a response with no payload or a payload of another encoding is refused and counted once as unprotected (req~ssr-e2e-refusal-counted), with no mismatch or missing_payload result or count and no wire given the payload."
  },
  "amended_by": {
    "record": "integration/composition/B3b-C08/RECORD-AMENDMENTS.json",
    "source": "W2-C08b-F01 (the row also takes the request route that composition batch 1 added to the clause, CCR-C06-20)"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/131/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/131",
  "row_number_1based": 132
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/131` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-132",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/131",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-240"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-133

req~ssr-integrity-checked-at-admission~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/132/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-integrity-checked-at-admission~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The admission check of an in-place frame covers the payload's size and the alignment of its address against the agreed layout, and its identity; each failing frame refused and counted.",
  "directive": "SM-09; CCR-C11-06",
  "cluster": "C08-integrity-peers-paths"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/132/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/132",
  "row_number_1based": 133
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/132` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-133",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/132",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-219"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-134

req~ssr-binding-iceoryx2-arrival-reports~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/133/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-arrival-reports~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "implementation_gap",
  "observation_case": "O1",
  "detail": "The O1 predicate needs a sender-side observation of whether a recipient's queue is non-empty at the beginning of the notification call; iceoryx2 0.9.3 offers none through its public publisher API (research R1 C1). Until the binding supplies and declares such an observation, it may not declare notification_call_failed for a route (the item's own rule), and the route is admitted or refused as its declared settings require. Corrective work: a declared queue-state mechanism with evidence, or no claim of the count.",
  "directive": "SM-07, D17, SR-214, IA-18",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/133/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/133",
  "row_number_1based": 134
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/133` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-134",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/133",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-212"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-135

req~ssr-binding-iceoryx2-arrival-reports~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/134/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-arrival-reports~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "implementation_gap",
  "observation_case": "O1",
  "detail": "listener_attempt_failed: iceoryx2 0.9.3 logs a failure on one listener's connection inside the notification call and returns the count of listeners notified, with no per-attempt result or callback (R1 B6). The binding cannot record the diagnostic from the public API; a mechanism (or an upstream change) is needed.",
  "directive": "SM-07, D17, SR-214",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/134/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/134",
  "row_number_1based": 135
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/134` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-135",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/134",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-212"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-136

req~ssr-binding-iceoryx2-arrival-reports~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/135/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-arrival-reports~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "implementation_gap",
  "observation_case": "O4",
  "detail": "Every arrival is owed a report within the declared latency, but a subscriber at iceoryx2 0.9.3 observes only whether any sample is queued and the samples it takes: no fill count, no eviction event, no refused-arrival event (R1 C2). With a queue that stays non-empty and a failed notification, an arrival has no evidence. The binding needs a mechanism that observes every required arrival at the queue or overflow boundary, or the route's claim is limited as qos and the binding's declaration require; no no-false-miss claim may rest on the subset observed. Two cases make the gap concrete: while one publisher's data connection is missing, arrivals of the route's other publishers into a queue that is already non-empty still need their reports; and a sample that was queued at the previous report, together with a later notification (for example of a publisher whose connection is not yet set up), is no evidence, so the binding needs a way to tell a newly queued sample from an old one.",
  "directive": "SM-07, D04, D11, D13, D17, SR-205",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/135/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/135",
  "row_number_1based": 136
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/135` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-136",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/135",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-212"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-137

req~ssr-binding-iceoryx2-degradation-handler~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/136/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-degradation-handler~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "implementation_gap",
  "observation_case": "O2",
  "detail": "The corrupted_connection key needs a connection generation that changes each time the connection to a recipient is set up again; iceoryx2 0.9.3 exposes no such generation, and its send error does not preserve the recipients reached (R1 C3, B7). The binding must keep its own generation per recipient and its own per-operation record to deduplicate and to keep the partial result.",
  "directive": "SM-07, XR-D179, D17",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/136/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/136",
  "row_number_1based": 137
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/136` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-137",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/136",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-213"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-138

req~ssr-binding-iceoryx2-degradation-handler~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/137/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-degradation-handler~2",
  "component": "the iceoryx2 binding at the historical pin 9e58439 (predecessor reader IR1, fable-successor-3/decisions/ICEORYX2-PROFILE.md section 3.1); the current binding revision is not assessed",
  "kind": "implementation_gap",
  "observation_case": "O2",
  "detail": "At the historical pin the degradation handler on each port was absent (IR1). Also unassessed: the send-route count of a failed connection and the corrupted_connection event, which this revision adds.",
  "directive": "SR-215, XR-D179",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/137/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/137",
  "row_number_1based": 138
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/137` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-138",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/137",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-213"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-139

req~ssr-binding-iceoryx2-static-segments~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/138/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-static-segments~2",
  "component": "the iceoryx2 binding at the historical pin 9e58439 (predecessor reader IR1, fable-successor-3/decisions/ICEORYX2-PROFILE.md section 3.1); the current binding revision is not assessed",
  "kind": "implementation_gap",
  "observation_case": "O8",
  "detail": "At the historical pin the binding made its own publishers static but recorded nothing on the service, required nothing of an opener and did not check the publishers present (IR1, M10). A static local publisher does not establish the service-wide property. Corrective work: the record (service attribute), the requirement when opening, the check of present publishers, each with a refusal at setup; evidence for each act.",
  "directive": "XR-D171, SR-04, SM-08",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/138/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/138",
  "row_number_1based": 139
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/138` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-139",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/138",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-210"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-140

req~ssr-binding-iceoryx2-panic-paths~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/139/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-panic-paths~4",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "New duties of this revision: the clock check at setup with its refusal, and the refusal of a configured suffix of the global management segment's name that leaves no room for the release's version (a suffix of 250 bytes or more with the default 255-byte limit, R1 B5). Until a binding revision is shown to make the suffix check, the binding's qualification record keeps that 0.9.3 path as a known failure (SR-216); the setup-time format! allocation of node creation goes to the record's setup-allocation entries.",
  "directive": "SR-216, XR-D177, SR-04",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/139/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/139",
  "row_number_1based": 140
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/139` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-140",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/139",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-209"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-141

req~ssr-binding-iceoryx2-panic-paths~4

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/140/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-panic-paths~4",
  "component": "the iceoryx2 binding's qualification record (QUAL-IOX)",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "The record needs 0.9.3 entries for every named path and for the further sites that the research found and the page does not name (R1 B4 vii: sender.rs 456-469 and 559-565, receiver.rs 235-247 and 310-312, the drop of a Unix datagram receiver, naming-scheme unwraps, dynamic-segment sites); older 0.9.2 entries keep their pins and are no evidence for 0.9.3 (SR-03). The accepted instance needs its known-anomaly entry at 0.9.3.",
  "directive": "XR-14, SR-271, SR-03, SR-205",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/140/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/140",
  "row_number_1based": 141
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/140` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-141",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/140",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-209"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-142

req~ssr-binding-iceoryx2-profile~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/141/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-profile~1",
  "component": "the iceoryx2 binding at the historical pin 9e58439 (predecessor reader IR1, fable-successor-3/decisions/ICEORYX2-PROFILE.md section 3.1); the current binding revision is not assessed",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "At the historical pin the core adapter's lock mixed six iceoryx2 crates at 0.9.2 with fifteen at 0.9.3, and the older crate was on 0.9.1 (predecessor record section 2); the profile may not be claimed for such a build. Refresh the full dependency closure at 0.9.3 (SR-03).",
  "directive": "SR-03, SR-341",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/141/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/141",
  "row_number_1based": 142
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/141` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-142",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/141",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-206"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-143

req~ssr-binding-depth-from-deployment~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/142/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-depth-from-deployment~2",
  "component": "the iceoryx2 binding at the historical pin 9e58439 (predecessor reader IR1, fable-successor-3/decisions/ICEORYX2-PROFILE.md section 3.1); the current binding revision is not assessed",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "iceoryx2 0.9.3 refuses an opener only an existing service whose largest subscriber queue is smaller; the binding at the historical pin named both depths only on the send side (IR1, M9). The binding's own comparison and refusal of any difference, naming the route and both depths, needs evidence on both start orders.",
  "directive": "SR-04",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/142/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/142",
  "row_number_1based": 143
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/142` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-143",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/142",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-207"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-144

req~ssr-binding-iceoryx2-event-ids~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/143/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-event-ids~1",
  "component": "the iceoryx2 binding at the historical pin 9e58439 (predecessor reader IR1, fable-successor-3/decisions/ICEORYX2-PROFILE.md section 3.1); the current binding revision is not assessed",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "At the historical pin the event IDs were not read from the event service's static config and a route whose lifecycle events share the data ID was not refused (IR1, M15).",
  "directive": "SR-04",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/143/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/143",
  "row_number_1based": 144
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/143` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-144",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/143",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-208"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-145

req~ssr-binding-iceoryx2-expired-buffer~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/144/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-expired-buffer~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "The factor two is sufficient against the borrowed-connection panic only while the route's retained budget bounds every sample borrowed from that subscriber (R1 B2: iceoryx2 bounds borrows per connection; a budget per route does not suffice if routes share a subscriber; unreturned or failed returns break the bound). The binding's premise (one subscriber per route, every borrow charged) needs evidence.",
  "directive": "SR-04, SR-205",
  "cluster": "C09-iceoryx2",
  "as_recorded_by_cluster": {
    "detail": "The factor two is sufficient against the borrowed-connection panic only while the route's retained budget bounds every sample borrowed from that subscriber (R1 B2: iceoryx2 bounds borrows per connection; a budget per route does not suffice if routes share a subscriber; unreturned or failed returns break the bound). The binding's premise (one subscriber per route, every borrow charged) needs evidence."
  },
  "amended_by": {
    "record": "integration/composition/B3b-L1X/RECORD-AMENDMENTS.json",
    "source": "W2-T8-F11"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/144/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/144",
  "row_number_1based": 145
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/144` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-145",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/144",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-211"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-146

req~ssr-qos-overflow~3

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/145/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-overflow~3",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "evidence_gap",
  "observation_case": "O3",
  "detail": "With safe overflow, iceoryx2 0.9.3 returns an evicted sample to the sender's side internally and tells neither the receiving application nor the public publisher API (R1 C2). A zero local discard counter does not prove zero middleware losses; the route's report must disclose the limitation, and a required lossless or complete-accounting claim is not established by the disclosure.",
  "directive": "SM-07, IA-16, IR-18",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/145/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/145",
  "row_number_1based": 146
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/145` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-146",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/145",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-152"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-147

req~ssr-binding-iceoryx2-frame-checks~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/146/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-frame-checks~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "The source-level review that the first seven checks precede any range formed from the header and that no byte of the byte region is read before they pass (VL-28; the Verification line now names it). iceoryx2 0.9.3 itself forms the slice of the received length with no bound (R1 A3), so the binding's order of checks is the only protection.",
  "directive": "VL-28",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/146/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/146",
  "row_number_1based": 147
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/146` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-147",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/146",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-201"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-148

req~ssr-binding-iceoryx2-frame-lifetime~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/147/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-frame-lifetime~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "The test case of a metadata record that does not cover exactly the metadata's extent, or whose token lies outside it (VL-28; added to the Verification line).",
  "directive": "VL-28",
  "cluster": "C09-iceoryx2",
  "as_recorded_by_cluster": {
    "detail": "The test case of a metadata record that does not cover exactly the metadata's extent, or whose token lies outside it (VL-28; added to the Verification line)."
  },
  "amended_by": {
    "record": "integration/composition/B3b-L1X/RECORD-AMENDMENTS.json",
    "source": "W2-T8-F11"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/147/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/147",
  "row_number_1based": 148
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/147` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-148",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/147",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-204"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-149

req~ssr-binding-iceoryx2-frame-layout~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/148/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-frame-layout~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The user-header type name that the binding records for the 40-byte header (iceoryx2 opens a service only for an equal name, size and variant, with no greater alignment) is a fact of each binding release for its qualification record; the profile fixes no name (DD-C09-02).",
  "directive": "SR-122, SR-03",
  "cluster": "C09-iceoryx2",
  "as_recorded_by_cluster": {
    "detail": "The user-header type name that the binding records for the 40-byte header (iceoryx2 opens a service only for an equal name, size and variant, with no greater alignment) is a fact of each binding release for its qualification record; the profile fixes no name (DD-C09-02)."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F09"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/148/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/148",
  "row_number_1based": 149
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/148` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-149",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/148",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-200"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-150

req~ssr-binding-readme~3

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/149/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "the iceoryx2 binding's qualification record (QUAL-IOX) and README",
  "kind": "evidence_gap",
  "observation_case": "O6",
  "detail": "iceoryx2 0.9.3 marks every channel of a dead port's connection closed before it cleans the shared memory (iceoryx2-cal/src/zero_copy_connection/common.rs lines 1081 to 1095), and a sender blocked on such a channel stops waiting. The record's default-cleanup entries, which cited 0.9.2, need targeted changed-path evidence at 0.9.3; the 0.9.2 results stay with their pins (SR-217). The README follow-on is BINDING-README-FOLLOW-ON.md.",
  "directive": "SR-217, SR-272",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/149/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/149",
  "row_number_1based": 150
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/149` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-150",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/149",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-151

req~ssr-binding-iceoryx2-degradation-handler~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/150/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-degradation-handler~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "implementation_gap",
  "observation_case": "O2",
  "detail": "iceoryx2 0.9.3 offers a degradation handler on publishers and subscribers and none on the notifier and listener ports of an event service (research R1 B7). On those ports the binding needs another declared means to learn of a connection that cannot be set up, and to apply its policy; none is established.",
  "directive": "SM-07, XR-D179; CHECK-SAFETY finding 5",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/150/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/150",
  "row_number_1based": 151
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/150` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-151",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/150",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-213"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-152

aou~seooc-iceoryx2-limits~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/151/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "aou~seooc-iceoryx2-limits~4",
  "component": "each deployment that uses the iceoryx2 binding (the integrator)",
  "kind": "unchecked",
  "observation_case": "O8",
  "detail": "The integrator's precondition that every program on a service that the binding uses creates its publishers with a static data segment carries req~ssr-binding-iceoryx2-static-segments~2 for publishers that join after setup; a deployment's evidence for it is assessed per deployment.",
  "directive": "XR-D171; CHECK-SAFETY finding 4",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/151/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/151",
  "row_number_1based": 152
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/151` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-152",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/151",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "deployment-records"
  ],
  "obligation_row_ids": [
    "RX03-OB-215"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-153

req~ssr-binding-iceoryx2-static-segments~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/152/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-static-segments~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "unchecked",
  "observation_case": "O8",
  "detail": "Admission of a route only under a declared, qualified configuration that establishes the service-wide static-segment property for every permitted publisher (later publishers of other programs included, under the integrator's declared precondition), with a refusal at setup that names the service; the deployment-data field that carries the declaration; and the configuration's qualification evidence. Not assessed at any binding revision or for any deployment.",
  "directive": "XR-D171; RECHECK-1 finding 2",
  "cluster": "C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/152/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/152",
  "row_number_1based": 153
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/152` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-153",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/152",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-210"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-154

req~ssr-binding-lola-envelope-layout~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/153/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-envelope-layout~2",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "Payload alignment in every mapping of a sample: no evidence that LoLa places a sample at the same address modulo the payload alignment in every process; the predecessor's reconciliation (binding commit 534b561, both paths) found writers padding for the writer's own mapping only. Until established, a route needing it is refused in route preparation.",
  "directive": "XR-12",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/153/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/153",
  "row_number_1based": 154
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/153` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-154",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/153",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-221"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-155

req~ssr-binding-lola-envelope-checks~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/154/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-envelope-checks~2",
  "component": "the LoLa binding",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Historical observation at binding commit 534b561c34b533d54303617d3ffe15dce8dd0adb (older transport path, src/frame.rs), recorded by the predecessor and not re-inspected: reserved bytes, a zero metadata length, lengths above the route's limits, an absent payload with a length, and payload alignment in the reader's mapping were not checked when read. The core-adapter path checked them. The refusal cause \"a refused envelope\", the return of a refused sample to LoLa and the order of the presence check are unchecked.",
  "directive": "XR-12, R1 of the predecessor envelope record",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/154/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/154",
  "row_number_1based": 155
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/154` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-155",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/154",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-222"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-156

req~ssr-binding-lola-envelope-routing-hint~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/155/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-envelope-routing-hint~2",
  "component": "the LoLa binding",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Historical observation at the predecessor's pins, not re-inspected: the older path wrote hint text as three path segments (SDK commit e7fbe175), and the core adapter compared hint bytes with stored text; the profile asks for the four-segment text, one canonical spelling written, and a parsing reader. Keep-or-count of a sample retained on its hint, with its own cause on the route that holds it, is unchecked.",
  "directive": "XR-12, R3 of the predecessor envelope record",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/155/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/155",
  "row_number_1based": 156
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/155` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-156",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/155",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-224"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-157

req~ssr-binding-lola-envelope-metadata~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/156/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-envelope-metadata~2",
  "component": "the element; the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "Source-level review that the binding reads nothing from the metadata but presence (VL-28), and evidence that the element has the selected wire decode the metadata and checks identity and filters before payload decoding for frames from LoLa. The metadata format item is requested from cluster C11 (CCR-C10-01); the SDK's current format document is provisional and is not the normative definition.",
  "directive": "XR-12, VL-28",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/156/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/156",
  "row_number_1based": 157
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/156` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-157",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/156",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-225"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-158

req~ssr-binding-lola-envelope-lifetime~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/157/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-envelope-lifetime~1",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Writable extent and view extent against the header; a view used after its sample is released (compile-fail where the API rules it out).",
  "directive": "XR-12",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/157/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/157",
  "row_number_1based": 158
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/157` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-158",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/157",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-226"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-159

req~ssr-binding-lola-profiles~3

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/158/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-profiles~3",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": "O10",
  "detail": "No accepted evidence that the binding's and LoLa's runtime paths, LoLa's lazy primitives included, acquire and release no heap memory; until it exists, each route declares allocation inside LoLa under a named permit and is admitted only under its waiver. The named setup-failure outcome is unchecked.",
  "directive": "XR-13, XR-15",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/158/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/158",
  "row_number_1based": 159
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/158` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-159",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/158",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-228"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-160

req~ssr-binding-lazy-forced~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/159/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lazy-forced~2",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": "O10",
  "detail": "On LoLa (pointed at by req~ssr-binding-lola-profiles~3, \"LoLa's first locks and lazy state\"): which first locks and lazy primitives of LoLa (for example function-local statics and lazily initialized locks) a runtime path of a route can reach at the revision built against, and whether the binding can take or force each before its transition returns without sending traffic; a primitive that it cannot force is an implementation gap against that item, and the route cannot be declared with the class none (req~ssr-binding-lola-profiles~3, \"Preparation\").",
  "directive": "XR-15, U198, C07-CCR-16",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/159/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/159",
  "row_number_1based": 160
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/159` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-160",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/159",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-182"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-161

req~ssr-binding-lola-capacity~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/160/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-capacity~2",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": "O7",
  "detail": "Which counts LoLa serves (transmit loans, retained samples, samples that LoLa keeps), the LoLa setting or storage each is taken from (for example the configured maximum number of subscribers and of elements per subscriber, and LoLa's pre-allocated monotonic memory pool), and whether each is an upper limit or guaranteed availability; how the binding establishes LoLa's storage at setup; that a request beyond a count is refused without asking LoLa.",
  "directive": "XR-13, RC-03 O7",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/160/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/160",
  "row_number_1based": 161
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/160` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-161",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/160",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-229"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-162

req~ssr-binding-lola-foreign-boundary~3

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/161/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-foreign-boundary~3",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": "O11",
  "detail": "Termination paths that the binding's calls into LoLa can reach, at the revision built against; for each claimed target profile, the builds and ways of loading for which nothing unwinds across the boundary (compiler versions and exception settings, unwind data, RTTI visibility, loader flags, runtime and unwinder composition; research R1 lists the dependencies, none established for a LoLa build); the mapping of LoLa's results to named refusals and counters. Catching exceptions in tests does not establish the absence of abort or terminate paths.",
  "directive": "XR-13, XR-14, SR-208, RC-03 O11",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/161/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/161",
  "row_number_1based": 162
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/161` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-162",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/161",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-230"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-163

req~ssr-binding-lola-callbacks~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/162/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-callbacks~2",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": "O11",
  "detail": "Termination assurance: what LoLa's threads can reach stays valid through each way the process ends (exit handlers, static and thread-local destruction, omitted stack destructors), and a mechanism, or a documented C-library guarantee, that excludes an exit from Rust code concurrent with an exit, quick exit or return from main in C or C++ code or in another copy of the Rust runtime; none is known (research R1). Whether any LoLa document names a no-use point for a callable; the callables' waits at source level.",
  "directive": "XR-13, SR-220, RC-03 O11",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/162/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/162",
  "row_number_1based": 163
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/162` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-163",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/162",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-232"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-164

aou~seooc-lola-assumptions-kept~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/163/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "aou~seooc-lola-assumptions-kept~1",
  "component": "the integrator",
  "kind": "unchecked",
  "observation_case": "O11",
  "detail": "Clause \"Exits\": no deployment record yet shows how a process that uses the LoLa binding excludes an exit, quick exit or return from main in Rust code at the same time as one in C or C++ code or in another copy of the Rust runtime, or which documented guarantee of the C library covers it.",
  "directive": "SR-220, XR-13",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/163/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/163",
  "row_number_1based": 164
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/163` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-164",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/163",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "deployment-records"
  ],
  "obligation_row_ids": [
    "RX03-OB-234"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-165

req~ssr-binding-lola-assumptions~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/164/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-assumptions~2",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The README's list of LoLa's applicable assumptions with keepers and means, at the revision built against: aou.trlc holds 38 records, 27 beside the eleven that the page lists, for example MonotonicSemiDynamicMemoryAllocation, CorrectlyConfiguredMaximumNumberOfSubscriber, CorrectlyConfiguredAsilLevel, DifferentUserForAsilAndQmProcesses and SameCompilerSettingsForProviderAndConsumerSide, and each that applies needs its keeper; the one-loan-per-producer-and-event limit declared as capability and reported as the binding's own; a non-recoverable allocation error returned and counted, never retried.",
  "directive": "XR-13, XR-22",
  "cluster": "C10-lola",
  "as_recorded_by_cluster": {
    "detail": "The README's list of LoLa's applicable assumptions with keepers and means, at the revision built against: aou.trlc holds 30 records beside the eight that the page lists, for example MonotonicSemiDynamicMemoryAllocation, CorrectlyConfiguredMaximumNumberOfSubscriber, CorrectlyConfiguredAsilLevel, DifferentUserForAsilAndQmProcesses and SameCompilerSettingsForProviderAndConsumerSide, and each that applies needs its keeper; the one-loan-per-producer-and-event limit declared as capability and reported as the binding's own; a non-recoverable allocation error returned and counted, never retried."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W1-C10a-F04"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/164/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/164",
  "row_number_1based": 165
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/164` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-165",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/164",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-233"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-166

req~ssr-binding-lola-sample-lifetime~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/165/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-sample-lifetime~1",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "No path that unsubscribes a proxy event, or destroys one or its proxy, while a sample is held, forgotten leases included.",
  "directive": "SR-05",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/165/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/165",
  "row_number_1based": 166
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/165` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-166",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/165",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-231"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-167

req~ssr-binding-lola-terms~1

Historical disposition: **annotation_only**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/166/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lola-terms~1",
  "component": "the LoLa binding",
  "kind": "annotation_only",
  "observation_case": null,
  "detail": "New item (needs uman, tag SafetyLola): the binding's manual has to cover it in the SDK trace record.",
  "directive": "SM-09",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/166/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/166",
  "row_number_1based": 167
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/166` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-167",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/166",
  "source_disposition": "annotation_only",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-227"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-168

aou~seooc-lola-assumptions-kept~1

Historical disposition: **annotation_only**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/167/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "aou~seooc-lola-assumptions-kept~1",
  "component": "the SDK safety-manual checker registry; the integrator",
  "kind": "annotation_only",
  "observation_case": null,
  "detail": "New assumption of use (tag SafetyManual): the safety-manual quotation registry of the SDK's checker needs the entry; this is tooling and holds no spec adoption (SR-277).",
  "directive": "SR-277, XR-13",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/167/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/167",
  "row_number_1based": 168
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/167` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-168",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/167",
  "source_disposition": "annotation_only",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "deployment-records"
  ],
  "obligation_row_ids": [
    "RX03-OB-234"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-169

req~ssr-binding-shared-state~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/168/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-shared-state~1",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": "O9",
  "detail": "Which of LoLa's ports write shared state that another process reads; until established the LoLa binding states that its ports write both ways, with one level per service.",
  "directive": "XR-22 (B4), RC-03 O9",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/168/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/168",
  "row_number_1based": 169
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/168` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-169",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/168",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-196"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-170

req~ssr-binding-integrity-level~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/169/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-integrity-level~1",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": "O9",
  "detail": "Whether a writer's level can be recorded where every process of a LoLa service reads it; until then the level is reported as unknown.",
  "directive": "RC-03 O9",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/169/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/169",
  "row_number_1based": 170
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/169` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-170",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/169",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-193"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-171

req~ssr-binding-peer-identity~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/170/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-peer-identity~1",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "What identifies a peer and its incarnation on LoLa (the binding reads no source from the metadata), or the statement that the binding cannot identify peers.",
  "directive": "XR-13",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/170/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/170",
  "row_number_1based": 171
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/170` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-171",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/170",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-185"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-172

req~ssr-binding-lent-storage~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/171/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-lent-storage~2",
  "component": "the LoLa binding",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "Whether LoLa has storage that can grow, move or be mapped again after the transition, and which setting or check the binding then needs (FU-04; LoLa's own notes describe a pre-allocated pool with monotonic allocation inside it).",
  "directive": "SR-15 (FU-04)",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/171/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/171",
  "row_number_1based": 172
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/171` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-172",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/171",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-195"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-173

aou~seooc-qualified-bindings-and-wires~9

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/172/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "aou~seooc-qualified-bindings-and-wires~9",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "No target profile is in the LoLa binding's claimed scope yet: evidence of the contract suites on real LoLa on any target is absent; evidence on iceoryx2 is none for LoLa.",
  "directive": "XR-22 (B6)",
  "cluster": "C10-lola"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/172/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/172",
  "row_number_1based": 173
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/172` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-173",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/172",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-199"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-174

req~ssr-native-type~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/173/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-native-type~3",
  "component": "the element (up-rust core, its derive and native route admission)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Show: a published statement of the types of size zero and the unions that the element admits, matching what its derive accepts; declared and reported size and alignment equal to the in-memory values, the size exact; no comparison, view or serialization that reads padding; the derive refusing a loan, a lease or a frame as a field.",
  "directive": "XR-09; XR-D196; XR-D194; XR-08",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/173/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/173",
  "row_number_1based": 174
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/173` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-174",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/173",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-235"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-175

req~ssr-native-agreement~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/174/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-native-agreement~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Show: each native frame sent with the identifier that the agreement names for its route; the agreement's layouts compared by the element (no declaration taken on trust); the agreement's ABI checked at setup.",
  "directive": "XR-D195; XR-X041; XR-08",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/174/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/174",
  "row_number_1based": 175
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/174` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-175",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/174",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-236"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-176

req~ssr-native-in-place~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/175/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-native-in-place~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "Show the element's own charging of a forgotten loan or frame for the life of the process, independent of the binding's storage duty, with no reliance on a destructor.",
  "directive": "XR-D187; XR-16",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/175/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/175",
  "row_number_1based": 176
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/175` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-176",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/175",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-237"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-177

req~ssr-wire-heap-claim-tested~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/176/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-heap-claim-tested~4",
  "component": "each serialized wire (among them the Protobuf wire and each wire of the XCDR2 profile)",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "Repeatable encoding now binds every serialized wire whatever its heap class. The Protobuf documentation warns that serialization need not be deterministic, so a Protobuf wire has to fix field order, packing, the presence of default values and the output of unknown fields, and show equal bytes for equal values; a heap-free claim has to hold over a decoded value's whole life (replacement and destruction).",
  "directive": "XR-D201; XR-D207; VL-22",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/176/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/176",
  "row_number_1based": 177
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/176` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-177",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/176",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred",
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-250"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-178

req~ssr-wire-metadata-format~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/177/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-metadata-format~1",
  "component": "each wire that a route selects to decode frame metadata (on LoLa and, per its candidate page, on iceoryx2)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "The metadata format is to be stated by the wire in its README at a fixed version (prefix, byte order, presence and encoding identifier, source and sink, token place); a pinned SDK document alone cannot be the normative definition (XR-12). Show the README's statement and the decode and report behaviour.",
  "directive": "XR-12; SM-13",
  "cluster": "C11-wires",
  "as_recorded_by_cluster": {
    "detail": "The metadata format is to be stated by the wire at a fixed version (prefix, byte order, presence and encoding identifier, source and sink, token place); a pinned SDK document alone cannot be the normative definition (XR-12). Show the statement and the decode and report behaviour."
  },
  "amended_by": {
    "record": "integration/composition/B3b-C11/RECORD-AMENDMENTS.json",
    "source": "W2-C11a-F02"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/177/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/177",
  "row_number_1based": 178
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/177` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-178",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/177",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-249"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-179

req~ssr-wire-no-panic~3

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/178/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-no-panic~3",
  "component": "each wire that a route selects to decode frame metadata",
  "kind": "evidence_gap",
  "observation_case": "",
  "detail": "The no-panic and error duties now cover the decode of a frame's metadata; the wire contract suite's fault injection has to include arbitrary metadata bytes.",
  "directive": "SM-13; XR-X032",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/178/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/178",
  "row_number_1based": 179
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/178` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-179",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/178",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-248"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-180

req~ssr-wire-encode-decode~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/179/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-encode-decode~3",
  "component": "each serialized wire, apart from the end-to-end protection wire",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Show: a truthful largest encoding (a Protobuf wire for a type with a string, bytes or repeated field states the format's cap below 2 GiB or a smaller limit that it enforces, not \"none\"); a stated smallest encoding honoured; a decoded value that refers to static or shared data still valid after its frame is returned and reused.",
  "directive": "XR-D202; SR-118; XR-D205",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/179/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/179",
  "row_number_1based": 180
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/179` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-180",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/179",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-246"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-181

req~ssr-wire-serialized-format~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/180/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-serialized-format~3",
  "component": "each serialized wire, apart from the end-to-end protection wire",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Show in each wire's statement that the type of a payload is the route-declared type, and that no successful decode is presented as proof of the sender's type.",
  "directive": "XR-22 B12",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/180/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/180",
  "row_number_1based": 181
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/180` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-181",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/180",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-245"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-182

req~ssr-wire-readme~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/181/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-readme~1",
  "component": "each wire",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "READMEs hold the lasting statements; facts that change per release, where kept, live in a record versioned with the wire.",
  "directive": "XR-13; XR-R068",
  "cluster": "C11-wires"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/181/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/181",
  "row_number_1based": 182
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/181` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-182",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/181",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-251"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-183

req~ssr-xcdr2-encoding~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/182/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-01",
  "requirement": "req~ssr-xcdr2-encoding~2",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Writes the predecessor format: prefix 06 00 00 00, no delimiter header, alignment up to eight from the first byte, no trailing padding. Required: 00 09, options with the trailing-padding count, little-endian delimiter header of the body length, alignment at most four from offset 0, zero padding in and after the body.",
  "directive": "XR-10; SR-262",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/182/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/182",
  "row_number_1based": 183
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/182` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-183",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/182",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-253"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-184

req~ssr-xcdr2-decoding~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/183/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-02",
  "requirement": "req~ssr-xcdr2-decoding~2",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Required: the six refusals (length at least eight; identifier 00 09; length = 8 + D + p; length a multiple of four; D the body length of an admitted writer type; every boolean of that type, skipped fields included), made before any field is read with arithmetic that cannot overflow; any padding and other options bits accepted.",
  "directive": "XR-10; XR-D153",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/183/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/183",
  "row_number_1based": 184
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/183` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-184",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/183",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-255"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-185

req~ssr-xcdr2-types~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/184/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-03",
  "requirement": "req~ssr-xcdr2-types~2",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The predecessor admitted arrays with a dimension of length zero and arrays of arrays. Required: refuse zero dimensions, duplicate field names, member identifiers other than the index and body lengths above 4 294 967 295, when built or at setup; the mapping keeps names, field types, order, defaults and mandatory marks.",
  "directive": "XR-11; SR-262",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/184/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/184",
  "row_number_1based": 185
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/184` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-185",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/184",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-252"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-186

req~ssr-xcdr2-length~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/185/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-04",
  "requirement": "req~ssr-xcdr2-length~2",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Required: exact encoded lengths of the new format, overflow-free size computation, refusal of a type whose encoded length the target cannot represent (32-bit targets), and the largest admitted input of a receiving end.",
  "directive": "XR-10",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/185/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/185",
  "row_number_1based": 186
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/185` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-186",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/185",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-254"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-187

req~ssr-xcdr2-evolution~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/186/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-05",
  "requirement": "req~ssr-xcdr2-evolution~1",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "No admitted writer types, setup compatibility check, defaults, tail skipping or equal-body-length refusal exist for the new profile; the route declaration input for admitted writer types is needed.",
  "directive": "XR-11",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/186/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/186",
  "row_number_1based": 187
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/186` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-187",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/186",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-256"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-188

req~ssr-xcdr2-identity~2

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/187/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-06",
  "requirement": "req~ssr-xcdr2-identity~2",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Required: state the version as the six items at their current revisions and the identifier that the deployment agreement defines; neither read, write, detect nor convert encodings of the fixed-layout XCDR2 profile; README statements of what agrees the type and what the format cannot detect.",
  "directive": "XR-10; XR-11; SM-16",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/187/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/187",
  "row_number_1based": 188
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/187` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-188",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/187",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-257"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-189

req~ssr-xcdr2-types~2, req~ssr-xcdr2-encoding~2, req~ssr-xcdr2-length~2, req~ssr-xcdr2-decoding~2, req~ssr-xcdr2-evolution~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/188/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-07",
  "requirement": "req~ssr-xcdr2-types~2, req~ssr-xcdr2-encoding~2, req~ssr-xcdr2-length~2, req~ssr-xcdr2-decoding~2, req~ssr-xcdr2-evolution~1",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "No vectors exist for the new profile. Unverified admitted types: the eleven primitive types, arrays of each primitive type with one, two and three dimensions, the type with no field. Missing cases: independent golden bytes; delimiter and length errors; alignment cases; trailing padding counts 0 to 3; non-zero padding and unknown options bits; old-to-new defaulting; new-to-old skipping with boolean checks; incompatible common-field changes; mandatory-member refusal at setup; same-length different types; bounded malformed routes (fault injection).",
  "directive": "XR-11 (last paragraph); SR-262",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/188/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/188",
  "row_number_1based": 189
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/188` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-189",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/188",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-252",
    "RX03-OB-253",
    "RX03-OB-254",
    "RX03-OB-255",
    "RX03-OB-256"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-190

req~ssr-xcdr2-identity~2

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/189/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-08",
  "requirement": "req~ssr-xcdr2-identity~2",
  "component": "the existing XCDR2 wire implementation of the SDK workspace and its derive (the implementation from which the predecessor fixture `06 00 00 00` was taken)",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "No test or review yet of the private-use identifier that a wire states against the deployment agreement that defines it.",
  "directive": "VL-28",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/189/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/189",
  "row_number_1based": 190
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/189` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-190",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/189",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "xcdr2-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-257"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-191

req~ssr-xcdr2-identity~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/190/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-C12-09",
  "requirement": "req~ssr-xcdr2-identity~2",
  "component": "each deployment and route that uses the fixed-layout XCDR2 profile",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Existing uses stay under revision 1 of the five items; moving a route to the new profile needs a declaration change at both ends. No inventory of existing uses has been made.",
  "directive": "XR-10; SM-16; SR-262",
  "cluster": "C12-xcdr2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/190/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/190",
  "row_number_1based": 191
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/190` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-191",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/190",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "deployment-records"
  ],
  "obligation_row_ids": [
    "RX03-OB-257"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-192

dsn~utransport-send-handed-over~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/191/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "dsn~utransport-send-handed-over~1",
  "component": "every UTransport implementation (uProtocol Transport Library) that claims the uP-L1 chapter",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The landed feature-branch correction gave this statement a key word (successful send only after handover to the underlying protocol's delivery mechanism); whether existing transport libraries meet the keyworded statement has not been assessed. No conformance is inferred; the key word's acceptance is the maintainers' (DR3-17, MAINTAINER-PROPOSALS README H6).",
  "directive": "DR3-17; RT-02; HD-10",
  "cluster": "C14-protocol-l1"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/191/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/191",
  "row_number_1based": 192
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/191` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-192",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/191",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-315"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-193

dsn~utransport-receive-filter-match~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/192/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "dsn~utransport-receive-filter-match~1",
  "component": "every UTransport implementation (uProtocol Transport Library) that claims the uP-L1 chapter",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The landed feature-branch correction gave this statement a key word (pull receive returns the least recent unexpired matching message; an omitted sink filter matches only messages without a sink); whether existing transport libraries meet the keyworded statement has not been assessed. No conformance is inferred; the key word's acceptance is the maintainers' (DR3-17, MAINTAINER-PROPOSALS README H6).",
  "directive": "DR3-17; RT-02; HD-10",
  "cluster": "C14-protocol-l1"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/192/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/192",
  "row_number_1based": 193
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/192` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-193",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/192",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-318"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-194

dsn~utransport-registerlistener-sink-filter~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/193/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "dsn~utransport-registerlistener-sink-filter~1",
  "component": "every UTransport implementation (uProtocol Transport Library) that claims the uP-L1 chapter",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The landed feature-branch correction gave this statement a key word (an omitted sink filter of registerListener matches only messages without a sink); whether existing transport libraries meet the keyworded statement has not been assessed. No conformance is inferred; the key word's acceptance is the maintainers' (DR3-17, MAINTAINER-PROPOSALS README H6).",
  "directive": "DR3-17; RT-02; HD-10",
  "cluster": "C14-protocol-l1"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/193/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/193",
  "row_number_1based": 194
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/193` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-194",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/193",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-329"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-195

req~sdk-owned-frame-contract~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/194/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-contract~1",
  "component": "up-rust SDK (core, application and asynchronous layers, std and alloc helpers); the published Cortex-M and RISC-V allocation providers; any scheduler integration or adapter",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "No component releases a declaration that names the owned-frame SDK contract, so no item of the area binds any of them until one does; the SDK that the area is written for needs the declaration (kinds, specification revision, offerings) and, where the safety area also binds it, both declarations. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/194/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/194",
  "row_number_1based": 195
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/194` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-195",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/194",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "api-async",
    "allocation-providers"
  ],
  "obligation_row_ids": [
    "RX03-OB-293"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-196

req~sdk-owned-frame-classic-apis~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/195/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-classic-apis~1",
  "component": "up-rust facade (classic UTransport/UMessage and Communication Layer APIs)",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "The classic APIs' requirement tests have not been run as R2 evidence with the owned-frame parts present and absent in one build and process; compatibility rounds check builds of consumers, not this observable. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/195/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/195",
  "row_number_1based": 196
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/195` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-196",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/195",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-295"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-197

req~sdk-owned-frame-adapter~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/196/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-adapter~1",
  "component": "up-rust SDK and Streamer packages",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Whether any shipped part carries messages between the classic APIs and owned-frame routes is not established; no part documents its crossings, and the compile-fail case for a loan, lease or retained frame standing where a classic message is required is not recorded. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/196/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/196",
  "row_number_1based": 197
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/196` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-197",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/196",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-296"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-198

req~sdk-owned-frame-extension-public~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/197/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-extension-public~1",
  "component": "up-rust asynchronous integration (the Tokio integration package)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Scheduler integration goes through hidden public hooks of the classic facade (route wake, wake, ttl, taken value and taken payload), not documented public interfaces. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/197/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/197",
  "row_number_1based": 198
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/197` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-198",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/197",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-304"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-199

req~sdk-owned-frame-extension-public~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/198/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-extension-public~1",
  "component": "up-rust SDK: selection of memory providers, bindings, wires and platform backends",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "Two allocation providers in separate repositories select through public interfaces, and the explicit platform seam takes a custom backend; whether a new public checked-alignment seam in the core is needed, and a review of each kind's documentation and registration path, are not recorded. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/198/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/198",
  "row_number_1based": 199
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/198` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-199",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/198",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "rtos-platform"
  ],
  "remaining_scope_portfolios": [
    "other-platform-backends",
    "allocation-providers"
  ],
  "obligation_row_ids": [
    "RX03-OB-304"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-200

req~sdk-owned-frame-provider-selection~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/199/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-provider-selection~1",
  "component": "up-rust alloc helper (allocator-aware constructors); up-rust std helper (host startup memory)",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "Review that each allocator-aware operation sends its requests to the selected provider and keeps its identity across transfers (MEM-03 acceptance) is not recorded; the std helper's documentation must state its provider, allocation domain and when the selection takes effect. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/199/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/199",
  "row_number_1based": 200
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/199` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-200",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/199",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-305"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-201

req~sdk-owned-frame-provider-contract~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/200/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-provider-contract~1",
  "component": "the published Cortex-M and RISC-V allocation providers; the up-rust std startup-memory helper",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "No provider publishes the seven-point statement (MEM-04: no provider contract record exists); the fixed providers do not restore capacity on release and retain issued backing, which their statement has to say. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/200/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/200",
  "row_number_1based": 201
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/200` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-201",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/200",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "allocation-providers"
  ],
  "obligation_row_ids": [
    "RX03-OB-306"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-202

req~sdk-owned-frame-provider-ownership~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/201/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-provider-ownership~1",
  "component": "up-rust alloc helper; the published allocation providers",
  "kind": "evidence_gap",
  "observation_case": "O6",
  "detail": "Release to the supplying provider or an equivalent handle, transfer with the provider's state beyond the creating object, calls only from supported contexts, and resize behaviour are not reviewed or tested for the SDK's allocator-aware paths. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed. Forgotten blocks held by layers above the core, the conditions of a block's validity while the SDK or an owner to which it passed the block holds it, and the use of a region only for the period that its source states, are not reviewed or tested either.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/201/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/201",
  "row_number_1based": 202
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/201` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-202",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/201",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "allocation-providers"
  ],
  "obligation_row_ids": [
    "RX03-OB-307"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-203

req~sdk-owned-frame-provider-failure~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/202/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-provider-failure~1",
  "component": "up-rust alloc and std helpers (layers above the core)",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Refusal results and documented dispositions of the layers' operations are not assessed; one helper returns the value and its allocator untouched on refusal (MEM-06 observation), the others are unassessed. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/202/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/202",
  "row_number_1based": 203
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/202` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-203",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/202",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-308"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-204

req~sdk-owned-frame-roles~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/203/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-roles~1",
  "component": "up-rust application-role layer",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Only encoded publishing exists; subscriber, notifier, notification listener, RPC client and server, and the native role family are missing. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/203/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/203",
  "row_number_1based": 204
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/203` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-204",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/203",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-297"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-205

req~sdk-owned-frame-handles~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/204/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-handles~1",
  "component": "up-rust application-role layer",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "The encoded publisher's handle is generic over its message type only; the other roles do not exist, and the one wire-selection interface is not reviewed. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/204/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/204",
  "row_number_1based": 205
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/204` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-205",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/204",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-298"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-206

req~sdk-owned-frame-defaults~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/205/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-defaults~1",
  "component": "up-rust application-role layer",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Documentation of each default at its point of use, and of setting dependencies, is not assessed. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/205/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/205",
  "row_number_1based": 206
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/205` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-206",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/205",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-299"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-207

req~sdk-owned-frame-layer-results~1

Historical disposition: **evidence_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/206/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-layer-results~1",
  "component": "up-rust application-role layer; asynchronous layer",
  "kind": "evidence_gap",
  "observation_case": null,
  "detail": "The encoded publisher passes partial-delivery distinctions on (AA-02); direct-core versus layer comparisons for every role and for asynchronous completion remain to be run. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed. Whether a layer above the core makes an allocation or a free of its own, or causes one in a memory provider beyond the route's heap class, in runtime mode on a route of the element is not assessed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame",
  "as_recorded_by_cluster": {
    "detail": "The encoded publisher passes partial-delivery distinctions on (AA-02); direct-core versus layer comparisons for every role and for asynchronous completion remain to be run. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed. Whether a layer above the core makes an allocation of its own, or causes one in a memory provider beyond the route's heap class, in runtime mode on a route of the element is not assessed."
  },
  "amended_by": {
    "record": "integration/composition/B3b-C16/RECORD-AMENDMENTS.json",
    "source": "W2-C16b-F01"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/206/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/206",
  "row_number_1based": 207
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/206` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-207",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/206",
  "source_disposition": "evidence_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-300"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-208

req~sdk-owned-frame-operation-kinds~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/207/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-operation-kinds~1",
  "component": "up-rust application-role layer",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "The publisher's first poll runs its synchronous send to completion and can wait in the binding or in an overflow wait: a progress step that waits. No operation documents its kind. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/207/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/207",
  "row_number_1based": 208
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/207` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-208",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/207",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-301"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-209

req~sdk-owned-frame-async-layer~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/208/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-async-layer~1",
  "component": "up-rust SDK",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "No executor-independent asynchronous layer exists; receive and RPC futures and their timers live in a Tokio-specific package that depends on the classic facade. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/208/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/208",
  "row_number_1based": 209
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/208` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-209",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/208",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-302"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-210

req~sdk-owned-frame-scheduler-integration~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/209/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-scheduler-integration~1",
  "component": "up-rust Tokio integration package",
  "kind": "implementation_gap",
  "observation_case": "O10",
  "detail": "Clause 'Resources': the integration publishes no resource statement for its wake-ups, timers and task ends. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "cluster": "C16-sdk-owned-frame",
  "as_recorded_by_cluster": {
    "detail": "A second receive on one route replaces the first task's waker (AA-07 observation), so a notification for a pending operation can be lost; the integration uses hidden hooks rather than public interfaces and publishes no resource statement for its wake-ups, timers and task ends. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-C16b-F06; B3b-RECORDS-CK-F4"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/209/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/209",
  "row_number_1based": 210
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/209` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-210",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/209",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-303"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-211

req~ssr-e2e-refusal-answered~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/210/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-answered~2",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Composition clarification (CCR-C06-23): at a server, each request that the E2E wire refuses is counted under the refusal's name (corrupt, undeclared data ID, or its kind), and that count is its one count in the E2E counter, with no second count. Not checked against any implementation. The cluster's implementation_gap row for this item already records that the E2E wire is not implemented in the current delivery.",
  "directive": "XR-18; HD-05; CCR-C06-23",
  "cluster": "composition:B2-C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/210/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/210",
  "row_number_1based": 211
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/210` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-211",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/210",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-127"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-212

req~ssr-streamer-config-validated~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/211/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-config-validated~1",
  "component": "the Streamer (crate and binary)",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Composition batch 2 added the clause 'Values only': assess the schema of the Streamer's deployment data, the worker settings that the Streamer binary applies included, for values only and nothing executed or interpreted (review of the schema). The Streamer's no-code duty has this item as its one home; req~ssr-config-no-code~2 binds the element only, for its own schema.",
  "directive": "CCR-C03-12; COMP-C03-NR1; XR-D107",
  "cluster": "composition:B2-C06-l2-streamer",
  "as_recorded_by_cluster": {
    "detail": "Composition batch 2 added the clause 'Values only': assess the schema of the Streamer's deployment data, the worker settings that the Streamer binary applies included, for values only and nothing executed or interpreted (review of the schema). Until C03 narrows req~ssr-config-no-code~2, the same duty is also assessed there."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-C06b-F06"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/211/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/211",
  "row_number_1based": 212
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/211` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-212",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/211",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-280"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-213

req~ssr-binding-unsafe-inventory~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/212/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-unsafe-inventory~2",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "The clause \"Generated code\" and the widened change detection require each binding's build and release verification to reach the unsafe sites that a generator emits into the binding, such as those that a middleware's derive of an unsafe trait emits for a type of the binding, each under a reviewed row of the binding's inventory or of the generator's. Code that a generator emits into the middleware or into the element is not in that reach. No binding's inventory check is known to cover generated code. (For example, the user header in the iceoryx2 protocol chapter derives iceoryx2's ZeroCopySend.)",
  "directive": "XR-06; XR-X034; W1-C07a-F03; B3C07-CK-01",
  "cluster": "composition:B3-C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/212/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/212",
  "row_number_1based": 213
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/212` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-213",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/212",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-194"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-214

req~ssr-binding-readme~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/213/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O9",
  "detail": "For a binding or a middleware with shared memory, the record must disclose the four shared-memory points; a label, matching layout, any-bits payload or private copy does not show safe middleware offsets or isolation.",
  "directive": "XR-17; W1-C07a-F02",
  "corrects": "clusters/C07-bindings/CONFORMANCE-FOLLOWUPS.json, the row for req~ssr-binding-readme~3 (O9, XR-17), whose detail opens \"For a middleware with shared memory\": the page's condition now reads \"Where a binding or its middleware uses shared memory\". The cluster record is not rewritten (HD-23); the consolidated obligation map takes this text.",
  "cluster": "composition:B3-C07-bindings"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/213/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/213",
  "row_number_1based": 214
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/213` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-214",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/213",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-215

req~ssr-binding-iceoryx2-static-segments~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/214/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-static-segments~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "unchecked",
  "observation_case": "O8",
  "detail": "Supersedes the detail of C09's unchecked O8 row for this requirement. The refusal at setup, naming the service, of a route on a service whose deployment data does not declare the integrator's precondition for later publishers (the deployment-data field that carries the declaration); and the qualification record's statement of each configuration for which the binding claims the service-wide property, with the producer and environment premises on which it rests for publishers that other programs add later. Not assessed at any binding revision or for any deployment.",
  "directive": "XR-D171; RC-03 O8; W1-C09a-F01",
  "cluster": "composition:B3-C09-iceoryx2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/214/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/214",
  "row_number_1based": 215
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/214` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-215",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/214",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-210"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-216

req~ssr-wire-metadata-format~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/215/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-metadata-format~1",
  "component": "each wire that a route selects to decode frame metadata, on the iceoryx2 binding's frames and on LoLa's",
  "kind": "unchecked",
  "observation_case": "",
  "detail": "Widens the scope of C11's unchecked row for this requirement (clusters/C11-wires/CONFORMANCE-FOLLOWUPS.json). For the metadata of a binding's frames, the stated format must also state each fact that a requirement specific to that binding fixes, as that requirement fixes it. For the iceoryx2 binding's profile, that means numbers least significant byte first, whatever the header's byte order (req~ssr-binding-iceoryx2-frame-layout~2). On both bindings, it means a payload encoding present exactly when the frame has a payload (req~ssr-binding-iceoryx2-frame-presence~1, req~ssr-binding-lola-envelope-presence~1). Show the stated format and the decode against those facts. No conformance is claimed.",
  "directive": "W1-ENV-F01; XR-12; SR-03; HD-10",
  "cluster": "composition:B3-ENV-WIRES-XCDR2"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/215/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/215",
  "row_number_1based": 216
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/215` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-216",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/215",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-249"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-217

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/216/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the Streamer, the Streamer binary, the supplier of the Streamer and the supplier of the Streamer binary",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "New item (composition batch 3b; HD-28, HD-30). Nobody in this correction inspected a Streamer or Streamer-binary source, inventory, generated output, release check or qualification record, so each of the four is unassessed against the item. Assess: the reviewed inventory of the unsafe sites of each part's own code (the Streamer's std loader and the Streamer binary's set-up and worker-creation code included), each row with its source-level argument, an empty reviewed inventory being a valid state; reassessment of a row when a change of safe code affects its obligation or argument; for code that a generator emits into either part and that the generator's rows cover, the account of the exact generator, configuration and inputs, and the inputs against the generator's constraints; each supplier's acceptance check over its part's own sources and generated output, with a negative control for a new, a changed and a removed site, and no release of a refused build; Miri over programs of safe calls through the Streamer's public API, its operator interface and its watchdog hook, alone and interleaved with forwarding, in initialization, at the transition and in runtime mode. A passing check or run is evidence of the named checks only; Miri does not run native middleware, FFI or platform code, which review and target evidence cover.",
  "directive": "XR-06; XR-04; RT-02; HD-28; HD-30; W2-T2-F01",
  "cluster": "composition:B3b-C06"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/216/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/216",
  "row_number_1based": 217
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/216` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-217",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/216",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-218

the dispatcher chapter's push-delivery rule (up-l2/dispatchers/README.adoc, 'If the uP-L1 delivery method is push: * SHALL provide an API to start/stop dispatching of UMessages per-topic'; proposed as dsn~dispatcher-push-flow-control~1 in MP-C06-12)

Historical disposition: **implementation_gap**.

Proposed / unmatched references; not adopted inventory entries: dsn~dispatcher-push-flow-control~1

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/217/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "the dispatcher chapter's push-delivery rule (up-l2/dispatchers/README.adoc, 'If the uP-L1 delivery method is push: * SHALL provide an API to start/stop dispatching of UMessages per-topic'; proposed as dsn~dispatcher-push-flow-control~1 in MP-C06-12)",
  "component": "the Streamer (crate and binary)",
  "kind": "implementation_gap",
  "observation_case": "",
  "detail": "Deliberate scoped difference (req~ssr-streamer-dispatch-obligations~3, clause 'The dispatcher chapter', third bullet): under the bounded-runtime Streamer profile a route's gate never returns to enabled in its process, so the Streamer offers no API to start and stop dispatching per topic and cannot also meet the unamended chapter rule. No conformance claim of the Streamer to that rule is possible until MP-C06-12, with its profile exception, is adopted (req~ssr-contract-transition~2, 'Where an item meets a chapter's rule', second case). Companion of the cluster's row for the three dispatcher rules of MP-C06-01 to MP-C06-03.",
  "directive": "SR-12; XR-21; HD-09; W2-C06b-F04",
  "cluster": "composition:B3b-C06"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/217/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/217",
  "row_number_1based": 218
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/217` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-218",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/217",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [],
  "join_exception": {
    "source_ref": "../conformance/INTAKE-INTEGRITY.json#/unresolved_inventory_references",
    "ids": [
      "dsn~dispatcher-push-flow-control~1"
    ]
  },
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-219

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/218/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element, on subscriber and notification-listener receive routes selecting the E2E wire",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance; unassessed. Before any payload wire, refuse a frame with no E2E-encoded payload, including absence and a different declared/inner identifier; no application value or permitted-absence report. Assess in every strict state with preceding checks admitted.",
  "directive": "HD-31 / Q1-A2",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/218/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/218",
  "row_number_1based": 219
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/218` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-219",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/218",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-220

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/219/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element, on subscriber and notification-listener receive routes selecting the E2E wire",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance; unassessed. Report each outer refusal as the fixed-size unprotected error variant under Named; distinguish it from an E2E wire header/CRC refusal.",
  "directive": "HD-31 / Q1-A1",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/219/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/219",
  "row_number_1based": 220
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/219` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-220",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/219",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-221

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/220/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element, on subscriber and notification-listener receive routes selecting the E2E wire",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Count the outer refusal once without heap in its own route counter; no E2E counter increment and no duplicate mismatch or missing_payload cause.",
  "directive": "HD-31 / Q1-A2",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/220/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/220",
  "row_number_1based": 221
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/220` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-221",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/220",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-222

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/221/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element, on subscriber and notification-listener receive routes selecting the E2E wire",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "implementation_conformance; unassessed. Release the newly covered refused frame at once under Calls and frames, retaining the applicable release/ownership rules.",
  "directive": "HD-31 / Q1 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/221/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/221",
  "row_number_1based": 222
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/221` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-222",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/221",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-223

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/222/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element, on subscriber and notification-listener receive routes selecting the E2E wire",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance; unassessed. An earlier refusal, including TTL, suppresses the later outer E2E refusal/count and invokes no wire; preserve the existing one-count ordering.",
  "directive": "HD-31 / Q1-A3 and Q1-A4b",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/222/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/222",
  "row_number_1based": 223
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/222` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-223",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/222",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-224

req~ssr-wire-encoding-checked~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/223/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-encoding-checked~2",
  "component": "the element, on subscriber and notification-listener receive routes selecting the E2E wire",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance; unassessed. Apply the explicit E2E replacement for mismatch, missing payload, present-empty dispatch under another identifier and permitted absence; keep ordinary typed send/non-E2E behavior and exclude opaque carriage. No inner-wire or Streamer duty is added.",
  "directive": "HD-31; B3b-C05-NR2",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/223/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/223",
  "row_number_1based": 224
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/223` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-224",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/223",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-240"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-225

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/224/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element integrated with the selected E2E wire on topic/notification receive routes",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance; unassessed. A present-empty payload carrying the E2E identifier, admitted by preceding checks, reaches the E2E wire and its header refusal is counted in the E2E counter, not unprotected. Existing detector/CRC evidence is separately scoped and earns no new integration credit.",
  "directive": "HD-31 / Q1-A4b",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/224/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/224",
  "row_number_1based": 225
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/224` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-225",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/224",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-226

req~ssr-qos-deadline~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/225/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-deadline~6",
  "component": "the element, on subscriber and notification-listener receive routes selecting the E2E wire",
  "kind": "unchecked",
  "observation_case": "O4",
  "detail": "implementation_conformance; unassessed. Check the existing queue-arrival contract together with the new refusal: a queued frame later refused as unprotected remains an arrival. This is an integration observation, not a new deadline duty or changed deadline definition.",
  "directive": "HD-31 / Q1-A4b",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/225/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/225",
  "row_number_1based": 226
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/225` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-226",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/225",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "e2e-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-153"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-227

req~ssr-heap-none~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/226/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-none~4",
  "component": "the element and each wire for its allocated native/shared work",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Preserve the causal no-acquisition/no-release rule across threads, later work and shared control; only the binding middleware permit/waiver operations and bound are excepted. Allocation by element code, wires or providers is not excused.",
  "directive": "HD-33 / Q3-1 to Q3-2a",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/226/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/226",
  "row_number_1based": 227
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/226` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-227",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/226",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-135"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-228

req~ssr-heap-none~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/227/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-none~4",
  "component": "the element and its composed native release path",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "implementation_conformance; unassessed. No heap acquisition/release during native lease drop, frame return, other release, retirement or deferred release, including caused middleware work and peer-change cleanup. The exception replaces no lazy-initialization, panic or memory-safety duty.",
  "directive": "HD-33 / Q3-2a, Q3-3, Q3-4",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/227/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/227",
  "row_number_1based": 228
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/227` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-228",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/227",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-135"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-229

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/228/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Derive/check the route class from every used wire direction, binding and allocation provider, including causal dependencies; a middleware-permit route is not class none and establishes no unqualified no-heap guarantee.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/228/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/228",
  "row_number_1based": 229
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/228` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-229",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/228",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-230

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/229/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Refuse native wire/provider causal allocation and binding-own-code allocation in a used direction even with a waiver; do not refuse solely this allocation entry for binding middleware-only allocation under its matching permit/waiver with all other conditions met.",
  "directive": "HD-33 / Q3-5 and Q3-7",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/229/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/229",
  "row_number_1based": 230
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/229` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-230",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/229",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-231

req~ssr-waiver-required~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/230/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-waiver-required~1",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. For native as well as other prepared middleware-allocation routes, require a matching named waiver for every binding permit before transition; otherwise name transport, route and permit in the refusal.",
  "directive": "HD-33 / Q3-2, Q3-6",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/230/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/230",
  "row_number_1based": 231
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/230` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-231",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/230",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-177"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-232

req~ssr-waiver-scope~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/231/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-waiver-scope~1",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Confine any admitted deviation to middleware operations and bound of the permit; no element/binding-own-code/wire/provider allocation, panic, native release exception or waiver of another operation requirement.",
  "directive": "HD-33 / Q3-2a and Q3-6",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/231/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/231",
  "row_number_1based": 232
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/231` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-232",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/231",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-178"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-233

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/232/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. A matching waiver does not cure a missing policy where this item requires one or an incompatible required guarantee. Keep the batch-3b policy scope; do not demand a policy from every declaration indiscriminately.",
  "directive": "HD-33; B3b-C04-NR1 / Q3-7",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/232/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/232",
  "row_number_1based": 233
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/232` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-233",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/232",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-234

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/233/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Report the actual admitted composed class and allocating actor, the declared policy including unknowns, and admitted middleware allocation separately by permit/operation as the corresponding evidence cases describe; no class-none relabelling.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/233/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/233",
  "row_number_1based": 234
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/233` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-234",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/233",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-235

req~ssr-binding-no-heap~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/234/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-no-heap~3",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Assess truthful class/permit declarations covering caused middleware work and the exact bounded middleware operations; no allocation in binding-own-code on a native path and no unqualified none claim for a permit route.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/234/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/234",
  "row_number_1based": 235
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/234` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-235",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/234",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-181"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-236

req~ssr-binding-frame-drop~10

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/235/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-frame-drop~10",
  "component": "the LoLa binding",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "implementation_conformance; unassessed. Assess native frame return and caused middleware release/retirement/deferred cleanup with no acquisition or release of heap; a matching middleware allocation waiver grants no exception to these release duties.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/235/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/235",
  "row_number_1based": 236
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/235` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-236",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/235",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "lola"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-183"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-237

req~ssr-binding-no-heap~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/236/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-no-heap~3",
  "component": "the iceoryx2 binding",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Assess truthful class/permit declarations covering caused middleware work and the exact bounded middleware operations; no allocation in binding-own-code on a native path and no unqualified none claim for a permit route.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/236/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/236",
  "row_number_1based": 237
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/236` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-237",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/236",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-181"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-238

req~ssr-binding-frame-drop~10

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/237/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-frame-drop~10",
  "component": "the iceoryx2 binding",
  "kind": "unchecked",
  "observation_case": "O6",
  "detail": "implementation_conformance; unassessed. Assess native frame return and caused middleware release/retirement/deferred cleanup with no acquisition or release of heap; a matching middleware allocation waiver grants no exception to these release duties.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/237/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/237",
  "row_number_1based": 238
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/237` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-238",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/237",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-183"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-239

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/238/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "each wire used on the native route",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Assess the entire causal acquisition/release scope of each declaration, including dependencies, later/threaded work and destruction. No own-code-only narrowing applies to this actor.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/238/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/238",
  "row_number_1based": 239
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/238` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-239",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/238",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-240

req~ssr-heap-declared~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/239/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-heap-declared~4",
  "component": "each allocation provider used on the native route",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Assess the entire causal acquisition/release scope of each declaration, including dependencies, later/threaded work and destruction. No own-code-only narrowing applies to this actor.",
  "directive": "HD-33 / Q3 records",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/239/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/239",
  "row_number_1based": 240
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/239` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-240",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/239",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "allocation-providers"
  ],
  "obligation_row_ids": [
    "RX03-OB-137"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-241

req~sdk-owned-frame-scheduler-integration~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/240/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-scheduler-integration~1",
  "component": "each scheduler integration",
  "kind": "unchecked",
  "observation_case": "O10",
  "detail": "implementation_conformance; unassessed. Assess the released documentation for heap release as well as unqualified allocation, wait and panic disclosures for registration, wake-ups, timers and task end. Include a task-end free without acquisition where present; disclose unknown as unknown. The declaration supplies no guarantee and this follow-up asserts no implementation defect.",
  "directive": "HD-35 (Q2); Q2.1 to Q2.4; B3b-C16-RN1",
  "cluster": "composition:B3c-A"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/240/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/240",
  "row_number_1based": 241
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/240` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-241",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/240",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-303"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-242

req~ssr-strict-ack-revision~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/241/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-ack-revision~3",
  "component": "the element's strict mode",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess exact-part/revision qualification input processing and the default affected-route set, including reliance through dependencies/shared state. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; D16; SM-02; XR-03",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/241/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/241",
  "row_number_1based": 242
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/241` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-242",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/241",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-105"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-243

req~ssr-strict-ack-revision~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/242/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-ack-revision~3",
  "component": "the element's strict mode",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess scope exclusion only where setup establishes exclusion from every recorded alternative, preserving each compound condition; unknown facts and acknowledgements exclude nothing. Include target-with-setting negative controls. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; D16; SM-11",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/242/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/242",
  "row_number_1based": 243
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/242` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-243",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/242",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-105"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-244

req~ssr-strict-ack-revision~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/243/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-ack-revision~3",
  "component": "the element's strict mode",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess Enforce/Locked refusal of the resulting affected routes regardless of acknowledgement, no refusal of a demonstrably unaffected route for that failure, and Report's absence of refusal for those findings. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; D16; SM-02",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/243/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/243",
  "row_number_1based": 244
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/243` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-244",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/243",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-105"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-245

req~ssr-strict-report~4

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/244/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-strict-report~4",
  "component": "the element's strict mode",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess the existing report's affected-route sets against the clarified scope definition, including nonbinding failures, compound scope and unknown setup facts; this item was not edited. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; D16; XR-03",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/244/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/244",
  "row_number_1based": 245
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/244` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-245",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/244",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-106"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-246

req~ssr-binding-readme~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/245/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess qualification-record use of the generalized limit/deviation/known-failure taxonomy against the requirements binding that part. Only bindings bear this item's record/README maintenance duties; the other parts are subjects of the definitions. No record schema or new maintenance duty for them is inferred. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; SM-11; XR-03; XR-13",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/245/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/245",
  "row_number_1based": 246
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/245` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-246",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/245",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-247

req~ssr-binding-readme~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/246/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "each wire",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess qualification-record use of the generalized limit/deviation/known-failure taxonomy against the requirements binding that part. Only bindings bear this item's record/README maintenance duties; the other parts are subjects of the definitions. No record schema or new maintenance duty for them is inferred. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; SM-11; XR-03; XR-13",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/246/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/246",
  "row_number_1based": 247
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/246` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-247",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/246",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-248

req~ssr-binding-readme~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/247/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess qualification-record use of the generalized limit/deviation/known-failure taxonomy against the requirements binding that part. Only bindings bear this item's record/README maintenance duties; the other parts are subjects of the definitions. No record schema or new maintenance duty for them is inferred. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; SM-11; XR-03; XR-13",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/247/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/247",
  "row_number_1based": 248
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/247` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-248",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/247",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-249

req~ssr-binding-readme~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/248/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-readme~3",
  "component": "each other selected part whose qualification record states a failure",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess qualification-record use of the generalized limit/deviation/known-failure taxonomy against the requirements binding that part. Only bindings bear this item's record/README maintenance duties; the other parts are subjects of the definitions. No record schema or new maintenance duty for them is inferred. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-32; SM-11; XR-03; XR-13",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/248/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/248",
  "row_number_1based": 249
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/248` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-249",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/248",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "selected-part-roster"
  ],
  "obligation_row_ids": [
    "RX03-OB-192"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-250

req~ssr-wire-scope~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/249/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-scope~2",
  "component": "the route declaration and its declared wires (definition domain)",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess declarations of a multi-wire composition naming the one metadata-decoding wire. This is the clarified declaration content, with no additional actor or software refusal invented. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "W2-C11a-F01; HD-35 Q1.1; DR2-8; XR-01",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/249/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/249",
  "row_number_1based": 250
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/249` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-250",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/249",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-247"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-251

req~ssr-wire-metadata-format~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/250/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-metadata-format~1",
  "component": "each selected metadata wire",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess existing format/decoding obligations using the canonical selection definition: the sole declared wire, or the declared composition member explicitly named for metadata. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "W2-C11a-F01; HD-35 Q1.1; XR-12; DR2-12",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/250/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/250",
  "row_number_1based": 251
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/250` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-251",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/250",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "additional-wires-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-249"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-252

req~ssr-binding-unsafe-inventory~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/251/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-unsafe-inventory~2",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess the exact generator/configuration/inputs/generated-output account for all emitted code with an unsafe site, irrespective of local-row use, including reviewed generator rows, source proofs and input constraints. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F01; XR-06",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/251/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/251",
  "row_number_1based": 252
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/251` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-252",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/251",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-194"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-253

req~ssr-binding-unsafe-inventory~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/252/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-unsafe-inventory~2",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess naming generator-owned rows by reference without duplicating their inventories. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F01; XR-06",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/252/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/252",
  "row_number_1based": 253
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/252` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-253",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/252",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-194"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-254

req~ssr-binding-unsafe-inventory~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/253/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-unsafe-inventory~2",
  "component": "each binding",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Establish the admitted inputs satisfy generator constraints and the proof's applicable invariants hold in the consuming build; inventory/site-change checks alone do not establish this. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F01; XR-06",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/253/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/253",
  "row_number_1based": 254
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/253` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-254",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/253",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "iceoryx2",
    "lola"
  ],
  "remaining_scope_portfolios": [
    "other-bindings"
  ],
  "obligation_row_ids": [
    "RX03-OB-194"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-255

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/254/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess the unconditional exact generator/configuration/inputs/output account and reviewed generator-owned rows with source proofs/input constraints for every generated unsafe site in this part's own code, including a site inventoried locally. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F01; XR-06; HD-30",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/254/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/254",
  "row_number_1based": 255
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/254` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-255",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/254",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-256

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/255/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess naming generator rows by reference without duplication and establishing input constraints and applicable consuming-build invariants. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F01; XR-06; HD-30",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/255/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/255",
  "row_number_1based": 256
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/255` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-256",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/255",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-257

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/256/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Establish source-level arguments for every row used to account for own-code unsafe, local or referenced; use the generator proof by reference for generated sites. Miri/tests/builds do not replace this argument. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F02; XR-06; HD-30",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/256/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/256",
  "row_number_1based": 257
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/256` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-257",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/256",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-258

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/257/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess reassessment of the row's application when any change affects the site's obligation or its argument in the part, including a consuming safe-helper change with unchanged unsafe text, generator/configuration/inputs/output and generator row. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F02; HD-30; HD28-ADJUDICATION dispositions 3, 4, 10",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/257/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/257",
  "row_number_1based": 258
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/257` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-258",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/257",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-259

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/258/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer binary",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess the unconditional exact generator/configuration/inputs/output account and reviewed generator-owned rows with source proofs/input constraints for every generated unsafe site in this part's own code, including a site inventoried locally. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F01; XR-06; HD-30",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/258/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/258",
  "row_number_1based": 259
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/258` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-259",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/258",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-260

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/259/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer binary",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess naming generator rows by reference without duplication and establishing input constraints and applicable consuming-build invariants. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F01; XR-06; HD-30",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/259/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/259",
  "row_number_1based": 260
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/259` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-260",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/259",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-261

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/260/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer binary",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Establish source-level arguments for every row used to account for own-code unsafe, local or referenced; use the generator proof by reference for generated sites. Miri/tests/builds do not replace this argument. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F02; XR-06; HD-30",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/260/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/260",
  "row_number_1based": 261
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/260` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-261",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/260",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-262

req~ssr-streamer-memory-safety~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/261/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-memory-safety~1",
  "component": "the supplier of the Streamer binary",
  "kind": "unchecked",
  "observation_case": "O12",
  "detail": "implementation_conformance: Assess reassessment of the row's application when any change affects the site's obligation or its argument in the part, including a consuming safe-helper change with unchanged unsafe text, generator/configuration/inputs/output and generator row. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "AFR1-F02; HD-30; HD28-ADJUDICATION dispositions 3, 4, 10",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/261/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/261",
  "row_number_1based": 262
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/261` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-262",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/261",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-270"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-263

req~ssr-wire-metadata-format~1

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/262/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-wire-metadata-format~1",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance: Assess metadata encoding for each originated frame against the route's selected wire's stated format using an independent reader; include fields, payload absence/present-empty/present-nonempty, a two-wire route and a wrong-format writer control. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-35 Q1.2; XR-12; DR2-12; XR-01; SM-13",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/262/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/262",
  "row_number_1based": 263
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/262` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-263",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/262",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-249"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-264

req~ssr-streamer-byte-identity~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/263/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-byte-identity~3",
  "component": "the element, for its carrier primitives",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance: Assess every re-encoded metadata instance for an egress selecting a metadata wire against that egress format; verify unchanged payload identifier/presence/bytes, opaque serialized carriage and a wrong-ingress-format control. No payload decoder installation or extra payload copy follows. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-35 Q1.5; XR-12; XR-18; SM-13",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/263/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/263",
  "row_number_1based": 264
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/263` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-264",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/263",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-273"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-265

req~ssr-streamer-byte-identity~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/264/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-streamer-byte-identity~3",
  "component": "the Streamer, for its routes",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "implementation_conformance: Assess every re-encoded metadata instance for an egress selecting a metadata wire against that egress format; verify unchanged payload identifier/presence/bytes, opaque serialized carriage and a wrong-ingress-format control. No payload decoder installation or extra payload copy follows. No implementation or qualification evidence was inspected in this authoring unit.",
  "directive": "HD-35 Q1.5; XR-12; XR-18; SM-13",
  "cluster": "composition:B3c-B"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/264/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/264",
  "row_number_1based": 265
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/264` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-265",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/264",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [],
  "remaining_scope_portfolios": [
    "streamer-deferred"
  ],
  "obligation_row_ids": [
    "RX03-OB-273"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-266

req~ssr-qos-ttl~6

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/265/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-qos-ttl~6",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Composition addition (CCR-C06-22): the checks of a TTL (expiry, an RPC request's TTL of 0, an unreadable clock) precede any check of the payload's encoding or presence at the same point, and a frame they refuse gets no later payload check, no missing_payload result and one count under that check's cause. Not checked against any implementation; the combined-fault Verification cases are new.",
  "directive": "XR-18; SM-04; HD-05; CCR-C06-22",
  "cluster": "composition:C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/265/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/265",
  "row_number_1based": 266
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/265` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-266",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/265",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-154"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-267

req~ssr-e2e-refusal-counted~3

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/266/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-e2e-refusal-counted~3",
  "component": "the element",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "Composition addition (CCR-C06-15): on an RPC server's request route that selects the E2E wire, a request with no E2E header (no payload, including where the operation permits absence; another encoding, including the inner wire's) is refused before any wire, not delivered, and counted once as unprotected in its own route counter. Not checked against any implementation; the E2E wire and its routes are recorded elsewhere in this cluster's follow-ups as not yet implemented in the current delivery.",
  "directive": "XR-X029; D01; HD-02; CCR-C06-15",
  "cluster": "composition:C05-qos-e2e"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/266/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/266",
  "row_number_1based": 267
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/266` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-267",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/266",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-126"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-268

req~ssr-contract-transition~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/267/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "id": "CF-HOST-RF23",
  "source": "R00 record-level row SR-279 (independent-review:RF23); records/record-level/ATTRIBUTION-CORRECTIONS.md entry RF23",
  "owner": "maintainers of each repository's trace tooling (coverage-tag importers)",
  "follow_up": "Establish whether any repository's coverage-tag importer read the example coverage tags `[utest->req~ssr-qos-ttl~4]` and `[impl->req~ssr-qos-ttl~4]` from the specification pages (they stood in safety/seooc-up-rust/README.adoc at the predecessor's s2, were removed by its edit VM01, and base 24eda44 holds neither). If one did, its configuration is corrected to read coverage tags only from implementation and test sources.",
  "blocks": "nothing in spec_adoption; an implementation_conformance check of the trace tooling only",
  "kind": "unchecked",
  "requirement": "req~ssr-contract-transition~2",
  "observation_case": "O12",
  "directive": "SR-279 (RF23)",
  "component": "the supplier of each part whose trace tooling imports coverage tags, for its record of where the part implements each item",
  "detail": "Establish whether any repository's coverage-tag importer read the example coverage tags `[utest->req~ssr-qos-ttl~4]` and `[impl->req~ssr-qos-ttl~4]` from the specification pages (they stood in safety/seooc-up-rust/README.adoc at the predecessor's s2, were removed by its edit VM01, and base 24eda44 holds neither). If one did, its configuration is corrected to read coverage tags only from implementation and test sources.",
  "cluster": "host",
  "as_recorded_by_cluster": {
    "detail": "Establish whether any repository's coverage-tag importer read the example coverage tags `[utest->req~ssr-qos-ttl~4]` and `[impl->req~ssr-qos-ttl~4]` from the specification pages (they stood in safety/seooc-up-rust/README.adoc at the predecessor's s2, were removed by its edit VM01, and base 24eda44 holds neither). If one did, its configuration is corrected to read coverage tags only from implementation and test sources."
  },
  "amended_by": {
    "record": "integration/composition/B3b-RECORDS/RECORD-AMENDMENTS.json",
    "source": "W2-T10-F10"
  }
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/267/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/267",
  "row_number_1based": 268
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/267` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-268",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/267",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2",
    "lola",
    "native-protobuf"
  ],
  "remaining_scope_portfolios": [
    "streamer-deferred",
    "selected-part-roster"
  ],
  "obligation_row_ids": [
    "RX03-OB-068"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-269

req~ssr-binding-iceoryx2-frame-lifetime~2

Historical disposition: **unchecked**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/268/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-binding-iceoryx2-frame-lifetime~2",
  "component": "the iceoryx2 binding (up-rust SEooC), at any revision; no binding revision was assessed by this correction",
  "kind": "unchecked",
  "observation_case": null,
  "detail": "No binding revision is assessed against the definition that revision 2 adds: that the retained budget to which the binding charges each received sample, and at which it takes no further sample, is the number of received frames that the route's declaration states (on a route of the Streamer, the retained budget of the route's declared budget), and that req~ssr-binding-iceoryx2-expired-buffer~2 checks twice that same number against the expired-connection buffer.",
  "directive": "SM-09; RT-02",
  "source": "W2-T8-F11",
  "cluster": "composition:B3b-L1X"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/268/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/268",
  "row_number_1based": 269
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/268` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-269",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/268",
  "source_disposition": "unchecked",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core",
    "iceoryx2"
  ],
  "remaining_scope_portfolios": [],
  "obligation_row_ids": [
    "RX03-OB-204"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-270

req~ssr-async-seam~4

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/269/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~ssr-async-seam~4",
  "component": "the asynchronous layer that implements the interface (the up-rust Tokio integration package)",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Property '2. Concurrent waiters': a second receive on one route replaces the first task's waker, and its drop clears the shared route storage (AA-07 observation at dc7f22d), so a pending operation's notification can be lost. No conformance is claimed.",
  "directive": "XR-07; RT-02; HD-10",
  "source": "W2-C16b-F06",
  "cluster": "composition:B3b-RECORDS"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/269/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/269",
  "row_number_1based": 270
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/269` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-270",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/269",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-100"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```

## RX03-FU-271

req~sdk-owned-frame-scheduler-integration~1

Historical disposition: **implementation_gap**.

### Exact recorded content

[Retained input](records/followups.json) · JSON pointer `/rows/270/source_row` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "requirement": "req~sdk-owned-frame-scheduler-integration~1",
  "component": "the up-rust SDK, for the Tokio integration package that it ships",
  "kind": "implementation_gap",
  "observation_case": null,
  "detail": "Clause 'Selection': the shipped integration uses hidden hooks rather than public, documented interfaces. Source: CLEANUP/../CRATE-LAYOUT.md (published SDK dc7f22da2bf90d98d94d468621df26caadf853ca); CLEANUP/../component-requirement-completion-1/ (MEM-xx, AA-xx observations at dc7f22d). No conformance is claimed.",
  "directive": "XR-23; RT-02; HD-10",
  "source": "W2-C16b-F06; B3b-RECORDS-CK-F4",
  "cluster": "composition:B3b-RECORDS"
}
```

### Recorded provenance

[Retained input](records/followups.json) · JSON pointer `/rows/270/source` · SHA256 `0b060d5f0adae138282fddc24f4cc3709f434f750b2e8eae81097b5b57fd423d`

```json
{
  "file": "sdk-api-redesign-20261107-1/delivery-continuation/shared-up-rust-core-20261114/up-spec-cleanup-1/fable-successor-4/package/conformance-followups/FOLLOWUPS.json",
  "json_pointer": "/270",
  "row_number_1based": 271
}
```

### Routing

[Retained input](records/routes.json) · JSON pointer `/rows/270` · SHA256 `0d6fa061a26e646732fb771d9540bf6683d8aebbafae0e650dd6be9c4e3088e4`

```json
{
  "id": "RX03-FU-271",
  "source_ref": "../conformance/FOLLOWUP-INTAKE.json#/rows/270",
  "source_disposition": "implementation_gap",
  "assessment_status": "not_reassessed",
  "existing_portfolios": [
    "sdk-core"
  ],
  "remaining_scope_portfolios": [
    "api-async"
  ],
  "obligation_row_ids": [
    "RX03-OB-303"
  ],
  "join_exception": null,
  "routing_basis": "unchanged source candidate routes; per-actor portfolio allocation, not conformance or expanded custody"
}
```
