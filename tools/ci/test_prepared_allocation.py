#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Negative controls for missing, duplicated and misleading allocation evidence."""
import unittest
from check_prepared_allocation import audit, SCENARIOS, ZERO_BENCH, CONTROL_BENCH


def harness():
    return '\n'.join(f'subject={name}\tallocations=0\tallocated_bytes=0\tall_threads=true' for name in sorted(SCENARIOS)) + '\nRESULT: all implemented prepared-path allocation subjects are zero\n'


def benchmark():
    rows = ['mode=bounded\titerations=10\telapsed_values_are_diagnostic_only=true',
            'metadata_work_retained=true\tfuture_polling=noop_waker']
    for _ in range(10):
        for name in sorted(ZERO_BENCH | CONTROL_BENCH):
            calls, size = (1, 4313) if name == 'heap_core_stable_send' else (0, 0)
            rows.append(f'subject={name}\tallocations={calls}\tallocated_bytes={size}\thost_performance_claim=false')
    return '\n'.join(rows)


class AllocationOutput(unittest.TestCase):
    def test_complete_harness(self):
        self.assertEqual(audit(harness()), len(SCENARIOS))

    def test_rejects_omission_duplicate_nonzero_bytes_and_partial_counter(self):
        full = harness()
        for bad in [full.split('\n', 1)[1], full + full.splitlines()[0],
                    full.replace('allocated_bytes=0', 'allocated_bytes=1', 1),
                    full.replace('all_threads=true', 'all_threads=false', 1),
                    full.replace('RESULT:', 'INTERRUPTED:')]:
            with self.subTest(bad=bad[:80]), self.assertRaises(ValueError):
                audit(bad)

    def test_benchmark_controls_and_full_iteration_count(self):
        full = benchmark()
        self.assertEqual(audit(full, True), 140)
        for bad in [full.replace('iterations=10', 'iterations=1'),
                    full.replace('subject=heap_core_stable_send\tallocations=1\tallocated_bytes=4313',
                                 'subject=heap_core_stable_send\tallocations=0\tallocated_bytes=0'),
                    full.replace('subject=prepared_loan\tallocations=0', 'subject=prepared_loan\tallocations=1', 1),
                    full.rsplit('\n', 1)[0]]:
            with self.subTest(bad=bad[:80]), self.assertRaises(ValueError):
                audit(bad, True)


if __name__ == '__main__':
    unittest.main()
