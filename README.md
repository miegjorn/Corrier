# Corrièr — protocol gateway for the Occitan stack

**Corrièr** (Occitan: *courier/postman*) is the gateway between human messaging
platforms and Nervi, the Occitan stack's agent substrate. It is the only component
in the stack allowed to hold a chat-platform credential. No agent process — Guilhem
or any component agent — ever logs into Matrix, Slack, or any other backend
directly; each agent's only communication substrate is Nervi.

Design: [`Occitan/docs/superpowers/specs/2026-07-04-corrier-matrix-nervi-gateway-design.md`](https://github.com/miegjorn/Occitan/blob/main/docs/superpowers/specs/2026-07-04-corrier-matrix-nervi-gateway-design.md).

## Shape

Two stateless gateways, not one bridge:

- **Read gateway** — receives inbound events from a protocol adapter (Matrix first,
  via an Application Service), resolves room→subject routing against Farga, and
  publishes to Nervi. No LLM calls, no session state, no direct agent invocation.
- **Write gateway** — subscribes to Nervi outbound chat subjects, renders and
  delivers the reply back through the originating protocol adapter.

Protocol-specific logic lives entirely inside an adapter implementing a shared
core contract (canonical message in, canonical reply out, plus per-adapter identity
provisioning). Matrix is the first adapter; Slack, WhatsApp, MS Teams, and IRC are
intended to follow the same contract.

Neither gateway holds a per-room map, a child process, or a model credential.
Continuity lives in Farga, not in this repo.

## Status

Design approved 2026-07-04, reviewed against Occitan's system-defence axioms
(Class 3 — architectural). Implementation plan in progress. This repo currently
contains scaffolding only.
