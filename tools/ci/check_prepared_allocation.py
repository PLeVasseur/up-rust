#!/usr/bin/env python3
# Copyright (c) 2026 Contributors to the Eclipse Foundation
# SPDX-License-Identifier: Apache-2.0
"""Run and audit full prepared-path allocation output, including missing rows."""
import argparse
from collections import Counter
import os
import re
import subprocess
import sys


SCENARIOS = {
    'prepared_route_send', 'direct_metadata_builder', 'reserved_route_creation',
    'prepared_receive_admit_and_borrow', 'received_header_parse',
    'token_set_clear_oversize', 'token_drop_and_wipe', 'closed_inbox_rejection',
    'full_inbox_rejection', 'generic_worker_send_admit_borrow_drain_stop',
    'rpc_first_contact_8_new_clients', 'rpc_max_clients_refusal',
    'rpc_token_multi_thread', 'rpc_token_1536_rotating', 'rpc_token_oversize_clear',
    'rpc_status_only_response', 'rpc_too_many_in_flight', 'rpc_late_response',
    'rpc_response_wrong_type', 'rpc_timeout', 'rpc_send_pool_exhausted',
    'rpc_closed_pending_and_new_call', 'publish_async_handler', 'publish_fanout_3',
    'publish_with_traceparent', 'notify_listener', 'publish_4k_payload',
    'publish_multi_thread_block_policy', 'send_pool_exhausted',
    'inbox_overflow_drop_newest', 'inbox_block_deadline_expiry',
    'send_token_and_traceparent_rejected', 'admission_wrong_payload_type',
    'admission_presence_length_alignment_bits_header', 'parse_mutated_headers',
    'wire_uri_rejection_variants', 'received_bad_authorities_rejected_in_place',
    'send_backend_storage_and_delivery_rejections', 'send_foreign_initialization_witness',
    'opaque_admit_forward_and_reject_reserved_origination', 'opaque_registered_forward_and_drain',
    'callback_admission_nested_pending_context',
    'repeating_array_initialization_and_rejection',
    'primary_rpc_sync_commit_and_retained_response',
    'first_send_unwarmed_route', 'first_receive_unwarmed_admit_and_borrow',
    'activation_and_first_traffic', 'peer_attach_detach_during_traffic',
}
ZERO_BENCH = {
    'prepared_loan', 'selected_wire_borrow', 'adapter_core_stable_send',
    'l2_stable_publish', 'core_only_loan_commit', 'direct_metadata_builder', 'in_place_receive',
}
CONTROL_BENCH = {
    'heap_core_stable_send', 'generated_uninitialized_init', 'initialized_loan_copy',
    'direct_memcpy_control', 'metadata_encode', 'metadata_decode', 'protobuf_message_projection',
}


def audit(text, bench=False):
    counts = Counter()
    for line in text.splitlines():
        if not line.startswith('subject='):
            continue
        fields = dict(part.split('=', 1) for part in line.split('\t'))
        name = fields['subject']
        calls, size = fields['allocations'], fields['allocated_bytes']
        try:
            released = fields['deallocations'], fields['deallocated_bytes']
            resized = fields['reallocations'], fields['reallocated_bytes']
        except KeyError as missing:
            raise ValueError(f'{name} is missing heap-use field {missing}') from None
        observed = (calls, size) + released + resized
        if not all(re.fullmatch(r'[0-9]+', value) for value in observed):
            raise ValueError('non-numeric allocation observation')
        counts[name] += 1
        if not bench or name in ZERO_BENCH:
            if any(int(value) for value in observed):
                raise ValueError(f'{name} used the heap: allocations={calls}/{size} '
                                 f'deallocations={released[0]}/{released[1]} '
                                 f'reallocations={resized[0]}/{resized[1]}')
        if bench:
            if fields.get('host_performance_claim') != 'false':
                raise ValueError('benchmark must remain a diagnostic')
            if name == 'heap_core_stable_send' and (int(calls) == 0 or int(size) == 0):
                raise ValueError('allocating heap control is missing its positive observation')
        elif fields.get('all_threads') != 'true':
            raise ValueError('all-thread observation required')
    expected = Counter({name: 10 for name in ZERO_BENCH | CONTROL_BENCH}) if bench else Counter(SCENARIOS)
    if counts != expected:
        raise ValueError(f'incomplete/duplicate subjects: expected={expected}, actual={counts}')
    if bench:
        if 'mode=bounded\titerations=10\t' not in text or 'metadata_work_retained=true\tfuture_polling=noop_waker' not in text:
            raise ValueError('full benchmark scope/completion missing')
    elif 'RESULT: all implemented prepared-path allocation subjects are zero' not in text:
        raise ValueError('allocation harness final completion missing')
    return sum(counts.values())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', required=True)
    parser.add_argument('--profile', choices=['debug', 'release'], default='debug')
    parser.add_argument('--bench', action='store_true')
    args = parser.parse_args()
    if 'UP_PAYLOAD_BENCH_SMOKE' in os.environ:
        parser.error('a smoke override cannot satisfy full allocation acceptance')
    command = ['cargo', '+' + args.toolchain, 'bench' if args.bench else 'test',
               '--locked', '-p', 'up-rust', '--all-features']
    if args.bench:
        if args.profile != 'release':
            command += ['--profile', 'dev']
        command += ['--bench', 'payload_contract']
    else:
        if args.profile == 'release':
            command.append('--release')
        command += ['--test', 'prepared_paths_zero_alloc']
    print(f'command={command!r}', flush=True)
    # The owning CI runner bounds and captures this entire process group. Preserve
    # stdout/stderr verbatim before auditing the successful child's result rows.
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    print(result.stdout, end='', flush=True)
    if result.returncode:
        return result.returncode
    try:
        observations = audit(result.stdout, args.bench)
    except (KeyError, ValueError) as error:
        print(f'FAIL allocation output: {error}', file=sys.stderr)
        return 1
    print(f'PASS full heap-use output: observations={observations}, profile={args.profile}, compiler={args.toolchain}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
