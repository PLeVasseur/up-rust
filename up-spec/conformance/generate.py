#!/usr/bin/env python3
"""Build the RX-03 documentary catalogue using only manifest-named metadata.
Documentary digests only; no product source/artifact reads, network or repository edits.
"""
import argparse
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUTPUTS = ("intake.v2.json", "index.html", "GENERATION-RECEIPT.json")


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def pointer(doc, path):
    for part in path.lstrip("/").split("/") if path else []:
        part = part.replace("~1", "/").replace("~0", "~")
        doc = doc[int(part)] if isinstance(doc, list) else doc[part]
    return doc


def check_input(item, raw):
    if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
        raise ValueError('Documentary input bytes changed: ' + item['id'])


def build(manifest, sources, sizes, template, manifest_digest=None):
    """Pure normalization/rendering from exact retained documentary records."""
    source_paths = {x["id"]: x["path"] for x in manifest["inputs"]}

    def ref(source, ptr=""):
        return {"source": source, "pointer": ptr}

    def original(source, ptr):
        return ref(source, ptr)

    spec = sources["adoption"]["specification"]
    sdk = sources["adoption"]["sdk"]
    fol = sources["followups"]["rows"]
    obs = sources["obligations"]["rows"]
    cases = sources["cases"]["cases"]
    route_rows = sources['routes']['rows']
    route_index = {row['id']: i for i, row in enumerate(route_rows)}
    if len(route_index) != len(route_rows) or set(route_index) != {f['id'] for f in fol}:
        raise ValueError('Routing IDs must be unique and match all follow-up IDs')
    routes = {row['id']: row for row in route_rows}
    source_dispositions = {}
    for f in fol:
        kind = f["source_row"]["kind"]
        source_dispositions[kind] = source_dispositions.get(kind, 0) + 1
    if source_dispositions != {"unchecked": 175, "evidence_gap": 44,
                               "annotation_only": 6, "implementation_gap": 46}:
        raise ValueError("Unexpected source dispositions; do not silently refresh the snapshot")

    assessments = {}
    def assessment(record_id):
        assessments[record_id] = {
            "implementation": "unassessed",
            "evidence": "unassessed",
            "applicability": "unknown",
            "component_revision": None,
            "target": None, "configuration": None, "profile": None,
            "accepted_evidence_refs": [],
            "claim": None,
        }
        return record_id

    def portfolios(followup_ids):
        result = set()
        for fid in followup_ids:
            r = routes[fid]
            result.update(r["existing_portfolios"])
            result.update(r["remaining_scope_portfolios"])
        return sorted(result)

    case_for = {f["id"]: [] for f in fol}
    normalized_cases = []
    for i, c in enumerate(cases):
        fids = list(dict.fromkeys(fid for join in c["source_row_joins"]
                                 for fid in join["followup_intake_ids"]))
        for fid in fids:
            case_for[fid].append(c["id"])
        normalized_cases.append({
            "id": c["id"], "label": c["case"].split(":", 1)[0],
            "source_ref": original("cases", f"/cases/{i}"),
            "followup_ids": fids, "case_ids": [c["id"]],
            "portfolio_ids": portfolios(fids),
            "assessment_id": assessment(c["id"]),
        })

    followups = []
    for i, f in enumerate(fol):
        s = f["source_row"]
        followups.append({
            "id": f["id"], "requirement": s["requirement"],
            "historical_source_disposition": s["kind"],
            "source_ref": original("followups", f"/rows/{i}/source_row"),
            "provenance_ref": ref("followups", f"/rows/{i}/source"),
            "routing_ref": ref("routes", f"/rows/{route_index[f['id']]}"),
            "portfolio_ids": portfolios([f["id"]]),
            "case_ids": case_for[f["id"]],
            "obligation_ids": routes[f["id"]]["obligation_row_ids"],
            "unmatched_requirement_refs": f["structured_references_not_in_obligation_inventory"],
            "assessment_id": assessment(f["id"]),
        })

    obligations = []
    for i, o in enumerate(obs):
        s = o["source_row"]
        ids = o["followup_intake_ids"]
        obligations.append({
            "id": o["id"], "requirement": s["requirement"],
            "page": s["page"], "source_change_class": s["change_class"],
            "source_ref": original("obligations", f"/rows/{i}/source_row"),
            "provenance_ref": ref("obligations", f"/rows/{i}/source"),
            "followup_ids": ids,
            "portfolio_ids": portfolios(ids),
            "case_ids": sorted({c for fid in ids for c in case_for[fid]}),
            "no_recorded_followup": not ids,
            "assessment_id": assessment(o["id"]),
        })

    identities = [
        {"id": "published-spec", "kind": "published_specification",
         "commit": spec["commit"], "tree": spec["tree"],
         "source_ref": ref("adoption", "/specification")},
        {"id": "published-sdk-documents", "kind": "published_documentary_pair",
         "commit": sdk["commit"], "tree": sdk["tree"],
         "mirror_equals_spec": sdk["mirror_exact"],
         "source_ref": ref("adoption", "/sdk"),
         "limitation": "Document publication identity; no new implementation or build qualification."},
        {"id": "historical-sdk-receipts", "kind": "historical_source_compatible_receipt_boundary",
         "commit": manifest["known_root_supplied_historical_identity"]["sdk_source_compatible_receipts"],
         "source_ref": {"source": "manifest", "pointer": "/known_root_supplied_historical_identity"},
         "limitation": "Retained results apply only to their original exact source and execution scope."},
        {"id": "source9-private", "kind": "private_unaccepted_candidate",
         "source_manifest": sources["source9-freeze"]["source9_pins"]["manifest_sha256"],
         "source_pins_record": sources["source9-freeze"]["source9_pins"]["sha256"],
         "source_ref": ref("source9-freeze", "/source9_pins")},
        {"id": "wi1-private", "kind": "private_unaccepted_candidate",
         "source_pins_record": sources["wire-handoff"]["repair"]["source_pins_sha256"],
         "historical_interface": sources["wire-handoff"]["source9"]["interface"],
         "historical_sdk_git_revision": sources["wire-handoff"]["source9"]["sdk_git_revision"],
         "source_ref": ref("wire-handoff", "/repair")},
        {"id": "f14-private", "kind": "private_unaccepted_candidate",
         "freeze_tree": sources["f14-freeze"]["tree"],
         "composed_staging_tree": sources["f14-pins"]["composed_f14"]["staging_tree"],
         "composed_adapter_tree_original_lock": sources["f14-pins"]["composed_f14"]["adapter_tree_with_original_lock"],
         "source_ref": ref("f14-pins", "/composed_f14"),
         "freeze_ref": ref("f14-freeze")},
        {"id": "d7-private", "kind": "private_unaccepted_candidate",
         "source_pins_record": sources["d7-manifest"]["files"]["SOURCE-PINS.json"]["sha256"],
         "frozen_utc": sources["d7-manifest"]["frozen_utc"],
         "source_ref": ref("d7-manifest", "/files/SOURCE-PINS.json"),
         "limitation": "Classification-2 composition only; previous review repair conditions remain."},
        {"id": "rtos5-helper-private", "kind": "private_unaccepted_component_candidate",
         "helper_derivative": sources["rtos-pins"]["frozen_helper"]["new"],
         "predecessor_helper": sources["rtos-pins"]["frozen_helper"]["old"],
         "whole_component_revision": None,
         "source_ref": ref("rtos-pins", "/frozen_helper"),
         "limitation": "Exact helper identity only. Narrow documentation-review acceptance does not establish RTOS component qualification."},
    ]
    names = {
        "SOURCE9": ("sdk-core", "SDK / Source9", ["published-sdk-documents", "historical-sdk-receipts", "source9-private"]),
        "WI1": ("native-protobuf", "Native + Protobuf / WI-1", ["wi1-private"]),
        "F14": ("iceoryx2", "iceoryx2 / F14", ["f14-private"]),
        "D7": ("lola", "LoLa / D7", ["d7-private"]),
        "RTOS5": ("rtos-platform", "MCU / RTOS5", ["rtos5-helper-private"]),
    }
    component_portfolios = []
    for packet, (pid, label, identity_ids) in names.items():
        p = sources["packet-" + packet]
        component_portfolios.append({
            "id": pid, "label": label, "identity_ids": identity_ids,
            "routing_state": "existing_portfolio",
            "supplier_identity": None, "integrator_identity": None,
            "current_accepted_implementation_revision": None,
            "active_custody": "not_established_by_this_catalogue",
            "supported_target": None, "configuration": None, "profile": None,
            "packet_ref": ref("packet-" + packet),
            "historical_owner_ref": ref("packet-" + packet, "/thread_identity_source"),
            "historical_review_boundary_ref": ref("packet-" + packet, "/frozen_history"),
        })
    for i, p in enumerate(sources["scopes"]["scope_portfolios"]):
        component_portfolios.append({
            "id": p["id"], "label": p["id"].replace("-", " ").title(),
            "identity_ids": [], "routing_state": p["routing_state"],
            "supplier_identity": None, "integrator_identity": None,
            "current_accepted_implementation_revision": None,
            "active_custody": "unresolved",
            "supported_target": None, "configuration": None, "profile": None,
            "packet_ref": ref("scopes", f"/scope_portfolios/{i}"),
        })
    transfers = [{
        "id": t["id"], "entry_key": t["entry_key"], "home": t["destination_home"],
        "source_ref": ref("transfers", f"/entries/{i}"),
        "portfolio_ids": [t["filing_portfolio"]] + t["consulted_scope_portfolios"],
        "filing_status": "pending",
        "navigation_followup_ids": t["same_cluster_followup_ids"],
        "navigation_limit": "Cluster-label match only; no requirement equivalence.",
    } for i, t in enumerate(sources["transfers"]["entries"])]
    source_table = [{"id": i["id"], "path": i["path"], "bytes_read": sizes[i["id"]],
                     "sha256": i['sha256'], "origin_locator": i['origin_locator']}
                    for i in manifest["inputs"]]
    source_table.append({"id": "manifest", "path": "INPUT-MANIFEST.v2.json",
                         "bytes_read": sizes["manifest"],
                         "sha256": manifest_digest or hashlib.sha256(encoded(manifest)).hexdigest()})
    data = {
        "schema": "rx03-normalized-conformance-intake/v2", "version": manifest["version"],
        "composition_owner": "root", "publication_state": "versioned_intake_catalogue",
        "specification_id": "published-spec", "specification_commit": spec["commit"],
        "publication_ref": ref("adoption"),
        "documentary_check_observation": {
            "source_ref": ref("checks"), "manual": sources["checks"]["manual"],
            "oft": sources["checks"]["oft"],
            "limitation": "Historical named checks, not current behavioral assessments. Not rerun here.",
        },
        "sources": source_table, "identities": identities, "component_portfolios": component_portfolios,
        "obligations": obligations, "followups": followups, "observation_cases": normalized_cases,
        "destination_transfers": transfers, "assessments": assessments,
        "scope": {
            "source_findings": "Exact retained records and pointers generate both the embedded viewer and repository-readable pages; no manually maintained duplicate findings.",
            "portfolio_links": "Routing candidates, not established component applicability or a closed selected-part roster.",
            "unlinked_obligations": "No follow-up is not a pass; all remain unassessed.",
            "tuple_applicability": "No target/configuration/profile or supported component tuple selected.",
            "root_role": "Catalogue composition only; not supplier/integrator.",
            "adoption": "Spec and SDK documents published. Implementation completion is not a spec-adoption prerequisite.",
            "historical": "Published documentation, old build receipts and private candidate identities are distinct.",
            "identities": "Read recorded identity strings only; no source or artifact was read or hashed.",
        },
    }
    checks = []

    def require(name, condition):
        if not condition:
            raise ValueError("Catalogue invariant failed: " + name)
        checks.append(name)

    require("504 unique obligation rows", len(obligations) == 504 == len({o["id"] for o in obligations}))
    require("226 safety page rows", sum(o["page"].startswith("safety/") for o in obligations) == 226)
    require("271 unique original followup rows", len(followups) == 271 == len({f["id"] for f in followups}))
    require("12 unique cases", len(normalized_cases) == 12 == len({c["id"] for c in normalized_cases}))
    require("328 no-followup obligations remain unassessed", sum(o["no_recorded_followup"] for o in obligations) == 328)
    require("90 transfers from 84 entries", len(transfers) == 90 and len({t["entry_key"] for t in transfers}) == 84)
    require("787 current assessments remain unassessed", len(assessments) == 787 and all(
        a["implementation"] == a["evidence"] == "unassessed"
        and a["applicability"] == "unknown"
        and all(a[k] is None for k in ("component_revision", "target", "configuration", "profile"))
        and not a["accepted_evidence_refs"] and a["claim"] is None for a in assessments.values()))
    require("source disposition and current dimensions independent", source_dispositions["annotation_only"] == 6
            and all("historical_source_disposition" in f for f in followups))
    known_f = {f["id"] for f in followups}
    known_o = {o["id"] for o in obligations}
    known_p = {p["id"] for p in component_portfolios}
    require("all followup joins resolve", all(set(o["followup_ids"]) <= known_f for o in obligations)
            and all(set(c["followup_ids"]) <= known_f for c in normalized_cases))
    require("all obligation and routing joins resolve", all(set(f["obligation_ids"]) <= known_o for f in followups)
            and all(set(r["portfolio_ids"]) <= known_p for r in obligations + followups + normalized_cases + transfers))
    require('877 globally unique record IDs', len({x['id'] for x in obligations + followups + normalized_cases + transfers}) == 877)
    require('all transfer navigation IDs resolve', all(set(t['navigation_followup_ids']) <= known_f for t in transfers))
    require('obligation/followup membership is reciprocal',
            {(o['id'], fid) for o in obligations for fid in o['followup_ids']} ==
            {(oid, f['id']) for f in followups for oid in f['obligation_ids']})
    require('case/followup membership is reciprocal',
            {(c['id'], fid) for c in normalized_cases for fid in c['followup_ids']} ==
            {(cid, f['id']) for f in followups for cid in f['case_ids']})
    require('routing provenance identifies the correct followup', all(
        pointer(sources['routes'], f['routing_ref']['pointer'])['id'] == f['id'] for f in followups))
    require("original order retained", [r["id"] for r in obligations] == [o["id"] for o in obs]
            and [r["id"] for r in followups] == [f["id"] for f in fol])
    require("unmatched dispatcher reference visible", any(f["id"] == "RX03-FU-218" and
            "dsn~dispatcher-push-flow-control~1" in f["unmatched_requirement_refs"] for f in followups))
    require("public docs and historical build identities differ", sdk["commit"] != identities[2]["commit"])
    require("explicit deferrals distinct", {"streamer-deferred", "e2e-deferred", "xcdr2-deferred",
                                          "additional-wires-deferred"} <= known_p)
    for row in obligations + followups + normalized_cases + transfers:
        pointer(sources[row["source_ref"]["source"]], row["source_ref"]["pointer"])
    require("all 877 authoritative row pointers resolve", True)
    def walk_refs(value):
        if isinstance(value, dict):
            if set(value) == {'source', 'pointer'}:
                yield value
            else:
                for child in value.values(): yield from walk_refs(child)
        elif isinstance(value, list):
            for child in value: yield from walk_refs(child)
    documents = dict(sources, manifest=manifest)
    for reference in walk_refs(data):
        pointer(documents[reference['source']], reference['pointer'])
    require('all row/provenance/routing/identity/portfolio references resolve', True)
    # One embedded authoritative snapshot supplies every generated excerpt/view.
    data['source_documents'] = documents
    compact = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    # Prevent source identifiers from closing the JSON script element.
    compact = compact.replace("&", "\\u0026").replace("<", "\\u003c").replace(">", "\\u003e")
    require("one JSON injection marker", template.count("__CATALOGUE_DATA__") == 1)
    page = template.replace("__CATALOGUE_DATA__", compact).encode()
    receipt = {
        "schema": "rx03-catalogue-generation-receipt/v1", "version": manifest["version"],
        "checks": checks, "status": "passed", "counts": {
            "obligations": 504, "followups": 271, "cases": 12, "transfers": 90,
            "unassessed_records": 787, "no_followup_obligations": 328, "current_passes": 0},
        "named_inputs": source_table,
        "input_content_bytes": sum(sizes.values()),
        "identity_verification": "Exact byte length and SHA256 for all19 bundled documentary inputs, plus named field guards; product identities remain historical unqualified strings.",
        "generated_payload_bytes": {"intake.v2.json": len(encoded(data)), "index.html": len(page)},
        "product_tests": 0, "source_or_artifact_hashing": False, "documentary_input_digests": True,
        "catalogue_independent_review": "pending root acceptance review",
        "catalogue_publication": "not performed",
    }
    outputs = {"intake.v2.json": encoded(data), "index.html": page,
               "GENERATION-RECEIPT.json": encoded(receipt)}
    outputs.update(markdown_views(data))
    return outputs


def markdown_views(data):
    """Repository navigation and exact derived excerpts; no manual assessment text."""
    def block(value):
        import re
        text = json.dumps(value, ensure_ascii=False, indent=2)
        fence = '`' * max(3, 1 + max((len(x) for x in re.findall(r'`+', text)), default=0))
        return fence + 'json\n' + text + '\n' + fence + '\n'
    docs, tables = data['source_documents'], {x['id']: x for x in data['sources']}
    output = {}
    for filename, title, rows in [
        ('obligations.md', 'Adopted obligation inventory', data['obligations']),
        ('followups.md', 'Historical follow-ups', data['followups']),
        ('cases.md', 'Observation cases', data['observation_cases']),
        ('transfers.md', 'Pending record transfers', data['destination_transfers'])]:
        parts = ['# ' + title, '\nGenerated from exact versioned records. Current implementation and evidence are unassessed; applicability is unknown. Historical findings remain open observations, not current conformance verdicts. Transfers record pending filing only.\n', '[Catalogue](README.md) · [Interactive view](index.html)\n']
        for row in rows:
            parts += ['## ' + row['id'], '\n' + (row.get('requirement') or row.get('label') or row['entry_key']) + '\n']
            if row.get('historical_source_disposition'):
                parts += ['Historical disposition: **' + row['historical_source_disposition'] + '**.\n']
            if row.get('unmatched_requirement_refs'):
                parts += ['Proposed / unmatched references; not adopted inventory entries: ' + ', '.join(row['unmatched_requirement_refs']) + '\n']
            if row.get('no_recorded_followup'):
                parts += ['No recorded follow-up; no pass inferred.\n']
            for key, label in [('source_ref','Exact recorded content'),('provenance_ref','Recorded provenance'),('routing_ref','Routing')]:
                if key not in row: continue
                ref=row[key]; src=tables[ref['source']]
                parts += ['### ' + label, '\n[Retained input](' + src['path'] + ') · JSON pointer `' + ref['pointer'] + '` · SHA256 `' + src['sha256'] + '`\n', block(pointer(docs[ref['source']],ref['pointer']))]
        output[filename] = ('\n'.join(parts).rstrip('\n') + '\n').encode()
    output['identities.md'] = ('# Recorded identities and portfolio boundaries\n\nThese are historical metadata observations, not current supplier, integrator, supported configuration or accepted component identities. Underlying evidence paths in the records are historical locators and are not bundled or requalified.\n\n[Catalogue](README.md)\n\n' + block(data['identities']) + '\n' + block(data['component_portfolios'])).encode()
    output['README.md'] = ('''# Conformance intake catalogue

This generated, versioned view accompanies the adopted up-spec revision
`f18913c6ed062dc6d6c3d7a73dec7a33433b39e4`. It does not change that specification.
The paired SDK documents are `0786fa6e3cd7dbe12c8c10a47d2b581914854b29`.

All current implementation and evidence assessments are **unassessed**;
applicability, component revision, target, configuration and profile remain
unknown. Historical implementation and evidence gaps remain visible. The 328
obligations without follow-ups are not passes. Portfolio filters select routing
candidates, not supported deployment tuples.

| Record view | Count |
| --- | ---: |
| [Adopted obligation inventory](obligations.md) | 504, including 226 safety-page rows |
| [Historical follow-ups](followups.md) | 271 |
| [Observation cases](cases.md) | 12 |
| [Pending transfers](transfers.md) | 90 destinations from 84 entries |

[Recorded identities](identities.md) distinguish public documents, historical
source-bound receipts and private unaccepted candidates. Streamer, E2E and
additional wires retain separate deferred scopes. Proposed references, including
FU-218's unmatched dispatcher identifier, are not adopted requirements.

The [interactive HTML](index.html) embeds all normalized data and the exact
documentary input snapshots it displays. Download/open it in a browser for
filters and pointer-aware record details; these Markdown pages work in repository
viewers without JavaScript. No separate site or server is required.

The [input manifest](INPUT-MANIFEST.v2.json) pins all 19 files under `records/`
by byte length and SHA256. The records are immutable documentary exports; their
product source/evidence locators are historical strings, not bundled or verified
evidence. Retaining a digest does not establish product conformance. Generated
excerpts and the HTML viewer derive from those records, not a manually maintained
second matrix. Held maintainer-proposal packets are not included or adopted.

Reproduce with `python3 -B generate.py --check`. All input bytes are verified in
the same required read pass before normalization. With absent outputs,
`python3 -B generate.py` creates them; existing outputs must already match.
Changed records require a deliberately new version, review and publication.
The generator reads only this directory, uses the Python standard library, and
runs no product tests. [Generation receipt](GENERATION-RECEIPT.json).

Manual/registry and OFT failures retain their original scope. Component
assessments, applicability and actual component-owned filings remain open.
This catalogue establishes no safety qualification or runtime guarantee.
''').encode()
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true",
                        help="Regenerate in memory and byte-compare only these catalogue outputs; no writes.")
    args = parser.parse_args()
    manifest_bytes = (HERE / "INPUT-MANIFEST.v2.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    if len({item['id'] for item in manifest['inputs']}) != len(manifest['inputs']):
        raise ValueError('Duplicate manifest input ID')
    sources, sizes = {}, {"manifest": len(manifest_bytes)}
    for item in manifest["inputs"]:
        path = (HERE / item["path"]).resolve()
        if not path.is_relative_to(HERE / 'records'):
            raise ValueError('Input escapes bundled records: ' + item['id'])
        raw = path.read_bytes()
        check_input(item, raw)
        sizes[item["id"]] = len(raw)
        sources[item["id"]] = json.loads(raw)
        for ptr, expected in item["expected"].items():
            if pointer(sources[item["id"]], ptr) != expected:
                raise ValueError("Named metadata identity changed: " + item["id"] + ptr)
    raw_template = (HERE / "template.html").read_bytes()
    sizes["template"] = len(raw_template)
    outputs = build(manifest, sources, sizes, raw_template.decode(), hashlib.sha256(manifest_bytes).hexdigest())
    if args.check:
        compared = 0
        for name, data in outputs.items():
            actual = (HERE / name).read_bytes()
            compared += len(actual)
            if actual != data:
                raise ValueError("Non-reproducible catalogue output: " + name)
        print(json.dumps({"mode": "check", "status": "passed", "deterministic_outputs": list(outputs),
                          "input_read_bytes": sum(sizes.values()), "own_output_read_bytes": compared}))
    else:
        for name, data in outputs.items():
            if (HERE / name).exists() and (HERE / name).read_bytes() != data:
                raise ValueError('Existing version output differs; create a deliberate successor: ' + name)
        for name, data in outputs.items():
            if not (HERE / name).exists(): (HERE / name).write_bytes(data)
        print(json.dumps({"mode": "generate", "status": "passed", "files": list(outputs),
                          "input_read_bytes": sum(sizes.values()),
                          "output_bytes": sum(map(len, outputs.values()))}))


if __name__ == "__main__":
    main()
