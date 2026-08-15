# Capability-Addressed Brains (CAB) Security Design

**Date:** 2026-07-20  
**Status:** Implemented (2026-07-22)  
**Scope:** HTTP serve auth, tenant locus, roles/caps, realm gating (issue #1)

## Thesis

Identity chooses a pure safe locus; tokens carry explicit caps; realms define trust.
Legitimate power stays available on purpose; none remains available by accident.

## Principles

1. Who / what / where are never one unsanitized string.
2. Authority is never inferred from absence or typos.
3. Trust is a place (embedded / loopback / served), not a loose flag.
4. Destructive memory ops are named powers (`govern`), not ambient Admin.
5. Every real workflow keeps a legal path.
6. `locus = F(brain_id)` is path-safe and contained under `tenants_root`.

## Model

| Old | CAB |
|-----|-----|
| `tenant_id` in FS path raw | Logical BrainId; disk = `tenants/<sha256-hex-32>/` (+ legacy safe-id dual-resolve) |
| `admin` crosses tenants | `admin` = `govern` on **bound** BrainId only |
| Control-plane via Admin | `platform` role = provision / revoke / list |
| Unknown role → Write/Admin | Unknown role → reject (no grant) |
| Open mode = Admin + any tenant | Open mode = Admin on BrainId `default` only |
| Non-localhost without keys | Already blocked; keep + strengthen |

## Role → caps

| Role | Caps |
|------|------|
| `read` | recall |
| `write` | recall + encode |
| `admin` | recall + encode + govern (same BrainId) |
| `platform` | provision only (not brain write via hierarchy) |

## Done when

Adversarial suite: no path escape; admin cannot cross tenant; unknown role no grant;
platform-only control plane; read cannot write; open mode cannot choose foreign locus.
