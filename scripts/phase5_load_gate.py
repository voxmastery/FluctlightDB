#!/usr/bin/env python3
"""Bounded HTTP load gate with explicit latency, error, and recovery thresholds."""

import argparse
import concurrent.futures
import json
import socket
import statistics
import time
import urllib.parse
import urllib.request


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int((len(ordered) - 1) * fraction))]


def request_once(url, timeout):
    started = time.monotonic()
    try:
        with urllib.request.urlopen(url, timeout=timeout) as response:
            response.read()
            status = response.status
    except Exception:
        status = 0
    return status, (time.monotonic() - started) * 1000


def open_slow_clients(host, port, count):
    clients = []
    partial = b"POST /api/v1/status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\n{"
    for _ in range(count):
        client = socket.create_connection((host, port), timeout=2)
        client.sendall(partial)
        clients.append(client)
    return clients


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:8787/live")
    parser.add_argument("--requests", type=int, default=2_000)
    parser.add_argument("--concurrency", type=int, default=32)
    parser.add_argument("--timeout-seconds", type=float, default=2)
    parser.add_argument("--minimum-success-rate", type=float, default=0.995)
    parser.add_argument("--maximum-p99-ms", type=float, default=500)
    parser.add_argument("--slow-clients", type=int, default=0)
    parser.add_argument("--maximum-recovery-ms", type=float, default=1_000)
    args = parser.parse_args()

    parsed = urllib.parse.urlsplit(args.url)
    slow = open_slow_clients(parsed.hostname, parsed.port or 80, args.slow_clients)
    started = time.monotonic()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        samples = list(
            pool.map(
                lambda _: request_once(args.url, args.timeout_seconds),
                range(args.requests),
            )
        )
    duration = time.monotonic() - started
    for client in slow:
        client.close()

    recovery_started = time.monotonic()
    recovery_status = 0
    while (time.monotonic() - recovery_started) * 1000 < args.maximum_recovery_ms:
        recovery_status, _ = request_once(args.url, args.timeout_seconds)
        if 200 <= recovery_status < 300:
            break
        time.sleep(0.01)
    recovery_ms = (time.monotonic() - recovery_started) * 1000
    latencies = [latency for _, latency in samples]
    successes = sum(200 <= status < 300 for status, _ in samples)
    success_rate = successes / max(1, len(samples))
    report = {
        "schema_version": 1,
        "requests": len(samples),
        "concurrency": args.concurrency,
        "throughput_requests_per_second": round(len(samples) / duration, 2),
        "success_rate": round(success_rate, 6),
        "latency_ms": {
            "mean": round(statistics.fmean(latencies), 3),
            "p50": round(percentile(latencies, 0.50), 3),
            "p95": round(percentile(latencies, 0.95), 3),
            "p99": round(percentile(latencies, 0.99), 3),
        },
        "slow_clients": args.slow_clients,
        "recovery_ms": round(recovery_ms, 3),
        "thresholds": {
            "minimum_success_rate": args.minimum_success_rate,
            "maximum_p99_ms": args.maximum_p99_ms,
            "maximum_recovery_ms": args.maximum_recovery_ms,
        },
    }
    report["passed"] = (
        success_rate >= args.minimum_success_rate
        and report["latency_ms"]["p99"] <= args.maximum_p99_ms
        and 200 <= recovery_status < 300
        and recovery_ms <= args.maximum_recovery_ms
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
