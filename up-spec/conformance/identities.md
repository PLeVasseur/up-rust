# Recorded identities and portfolio boundaries

These are historical metadata observations, not current supplier, integrator, supported configuration or accepted component identities. Underlying evidence paths in the records are historical locators and are not bundled or requalified.

[Catalogue](README.md)

```json
[
  {
    "id": "published-spec",
    "kind": "published_specification",
    "commit": "f18913c6ed062dc6d6c3d7a73dec7a33433b39e4",
    "tree": "29c6019918c7527a6fa8f4ed3a82d6690bea3dfa",
    "source_ref": {
      "source": "adoption",
      "pointer": "/specification"
    }
  },
  {
    "id": "published-sdk-documents",
    "kind": "published_documentary_pair",
    "commit": "0786fa6e3cd7dbe12c8c10a47d2b581914854b29",
    "tree": "82a7621a70ce062e23376c1f5bf060cfe2c0b820",
    "mirror_equals_spec": true,
    "source_ref": {
      "source": "adoption",
      "pointer": "/sdk"
    },
    "limitation": "Document publication identity; no new implementation or build qualification."
  },
  {
    "id": "historical-sdk-receipts",
    "kind": "historical_source_compatible_receipt_boundary",
    "commit": "dc7f22da2bf90d98d94d468621df26caadf853ca",
    "source_ref": {
      "source": "manifest",
      "pointer": "/known_root_supplied_historical_identity"
    },
    "limitation": "Retained results apply only to their original exact source and execution scope."
  },
  {
    "id": "source9-private",
    "kind": "private_unaccepted_candidate",
    "source_manifest": "6e5b867e7f7812790e26ea0cf60dc9d1b83c5d43d99adc0fe606e024b85f6a4d",
    "source_pins_record": "ee18145ded0a64eed33eb23db6944a76e3d91894dab8a60000495ab1093c13a7",
    "source_ref": {
      "source": "source9-freeze",
      "pointer": "/source9_pins"
    }
  },
  {
    "id": "wi1-private",
    "kind": "private_unaccepted_candidate",
    "source_pins_record": "911dd09078ccf4f3e78f83bd40c86fca7338d64cfe1471941d1067f21e919979",
    "historical_interface": "S9-WI-1",
    "historical_sdk_git_revision": "8e386c12f2d7a3441fb0ff7216df5ef7ea331161",
    "source_ref": {
      "source": "wire-handoff",
      "pointer": "/repair"
    }
  },
  {
    "id": "f14-private",
    "kind": "private_unaccepted_candidate",
    "freeze_tree": "0894483db84fdbdd9424b9696be3b7806b6adf9872fdc4a0838a584fb5ac5e0b",
    "composed_staging_tree": "41bd5ed2e1d7a1b07b3a35027b2b7bc8debae882b4648fd30fb5162764d305b8",
    "composed_adapter_tree_original_lock": "a98d18bc8dfc78aa2d739da52f6ab70dc6623fd6fd3510b6e3d5cb87d74e89a5",
    "source_ref": {
      "source": "f14-pins",
      "pointer": "/composed_f14"
    },
    "freeze_ref": {
      "source": "f14-freeze",
      "pointer": ""
    }
  },
  {
    "id": "d7-private",
    "kind": "private_unaccepted_candidate",
    "source_pins_record": "f033d1d557b8bd4e79c83fb70bdbbcd904e156637e5ab9b391b276eba53132e7",
    "frozen_utc": "2026-11-16T23:27:59Z",
    "source_ref": {
      "source": "d7-manifest",
      "pointer": "/files/SOURCE-PINS.json"
    },
    "limitation": "Classification-2 composition only; previous review repair conditions remain."
  },
  {
    "id": "rtos5-helper-private",
    "kind": "private_unaccepted_component_candidate",
    "helper_derivative": "073d18f03f3662f3b2d97105a84274e47d65b13889dbcc8bf0b694b915b191e9",
    "predecessor_helper": "d19e45c07f536e24f4e778925fa88b0e06bc62d0b759060ed2110613b90887bc",
    "whole_component_revision": null,
    "source_ref": {
      "source": "rtos-pins",
      "pointer": "/frozen_helper"
    },
    "limitation": "Exact helper identity only. Narrow documentation-review acceptance does not establish RTOS component qualification."
  }
]
```

```json
[
  {
    "id": "sdk-core",
    "label": "SDK / Source9",
    "identity_ids": [
      "published-sdk-documents",
      "historical-sdk-receipts",
      "source9-private"
    ],
    "routing_state": "existing_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "not_established_by_this_catalogue",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "packet-SOURCE9",
      "pointer": ""
    },
    "historical_owner_ref": {
      "source": "packet-SOURCE9",
      "pointer": "/thread_identity_source"
    },
    "historical_review_boundary_ref": {
      "source": "packet-SOURCE9",
      "pointer": "/frozen_history"
    }
  },
  {
    "id": "native-protobuf",
    "label": "Native + Protobuf / WI-1",
    "identity_ids": [
      "wi1-private"
    ],
    "routing_state": "existing_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "not_established_by_this_catalogue",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "packet-WI1",
      "pointer": ""
    },
    "historical_owner_ref": {
      "source": "packet-WI1",
      "pointer": "/thread_identity_source"
    },
    "historical_review_boundary_ref": {
      "source": "packet-WI1",
      "pointer": "/frozen_history"
    }
  },
  {
    "id": "iceoryx2",
    "label": "iceoryx2 / F14",
    "identity_ids": [
      "f14-private"
    ],
    "routing_state": "existing_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "not_established_by_this_catalogue",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "packet-F14",
      "pointer": ""
    },
    "historical_owner_ref": {
      "source": "packet-F14",
      "pointer": "/thread_identity_source"
    },
    "historical_review_boundary_ref": {
      "source": "packet-F14",
      "pointer": "/frozen_history"
    }
  },
  {
    "id": "lola",
    "label": "LoLa / D7",
    "identity_ids": [
      "d7-private"
    ],
    "routing_state": "existing_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "not_established_by_this_catalogue",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "packet-D7",
      "pointer": ""
    },
    "historical_owner_ref": {
      "source": "packet-D7",
      "pointer": "/thread_identity_source"
    },
    "historical_review_boundary_ref": {
      "source": "packet-D7",
      "pointer": "/frozen_history"
    }
  },
  {
    "id": "rtos-platform",
    "label": "MCU / RTOS5",
    "identity_ids": [
      "rtos5-helper-private"
    ],
    "routing_state": "existing_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "not_established_by_this_catalogue",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "packet-RTOS5",
      "pointer": ""
    },
    "historical_owner_ref": {
      "source": "packet-RTOS5",
      "pointer": "/thread_identity_source"
    },
    "historical_review_boundary_ref": {
      "source": "packet-RTOS5",
      "pointer": "/frozen_history"
    }
  },
  {
    "id": "streamer-deferred",
    "label": "Streamer Deferred",
    "identity_ids": [],
    "routing_state": "deliberately_deferred",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/0"
    }
  },
  {
    "id": "api-async",
    "label": "Api Async",
    "identity_ids": [],
    "routing_state": "existing_pending_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/1"
    }
  },
  {
    "id": "e2e-deferred",
    "label": "E2E Deferred",
    "identity_ids": [],
    "routing_state": "deliberately_deferred",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/2"
    }
  },
  {
    "id": "xcdr2-deferred",
    "label": "Xcdr2 Deferred",
    "identity_ids": [],
    "routing_state": "separate_deferred_wire_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/3"
    }
  },
  {
    "id": "allocation-providers",
    "label": "Allocation Providers",
    "identity_ids": [],
    "routing_state": "existing_pending_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/4"
    }
  },
  {
    "id": "other-platform-backends",
    "label": "Other Platform Backends",
    "identity_ids": [],
    "routing_state": "existing_pending_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/5"
    }
  },
  {
    "id": "additional-wires-deferred",
    "label": "Additional Wires Deferred",
    "identity_ids": [],
    "routing_state": "separate_deferred_wire_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/6"
    }
  },
  {
    "id": "other-bindings",
    "label": "Other Bindings",
    "identity_ids": [],
    "routing_state": "existing_pending_portfolio",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/7"
    }
  },
  {
    "id": "deployment-records",
    "label": "Deployment Records",
    "identity_ids": [],
    "routing_state": "root_owned_applicability_work",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/8"
    }
  },
  {
    "id": "selected-part-roster",
    "label": "Selected Part Roster",
    "identity_ids": [],
    "routing_state": "root_owned_applicability_work",
    "supplier_identity": null,
    "integrator_identity": null,
    "current_accepted_implementation_revision": null,
    "active_custody": "unresolved",
    "supported_target": null,
    "configuration": null,
    "profile": null,
    "packet_ref": {
      "source": "scopes",
      "pointer": "/scope_portfolios/9"
    }
  }
]
```
