# i2pr-sam Planning System

This repository uses the CodeGG planning model: stable long-term direction is separated
from subsystem roadmaps, bounded implementation handoffs, and closure evidence.

## Canonical long-term documents

- `000-long-term-specification.md` — normative end-state and invariants.
- `001-terminology-and-domain-model.md` — stable terminology and identity model.
- `002-long-term-roadmap.md` — dependency-ordered capability roadmap.
- `003-planning-process.md` — planning/closure rules for this repository.

Ordinary implementation work must not silently rewrite these documents. Material
architecture changes require an ADR or an explicit long-term-specification update.

## Planning hierarchy

```text
long-term specification + terminology
        |
        v
architecture decisions
        |
        v
subsystem roadmaps
        |
        v
milestone implementation plans
        |
        v
implementation + verification
        |
        v
closure records
```

## Directory roles

- `adrs/` — durable architectural decisions.
- `subsystems/` — dependency-aware workstream roadmaps.
- `implementation/` — bounded implementation-agent handoffs.
- `closure/` — authoritative evidence and residual-risk records.
- `archive/` — superseded planning retained for traceability.
- `registry.md` — compact active control surface.

## Classification

Every roadmap and implementation plan distinguishes:

- **Invariant** — must remain true.
- **Capability** — user/developer/operator-visible behavior.
- **Infrastructure** — machinery required by capabilities.
- **Polish** — ergonomics, diagnostics, performance, docs, or cleanup.

Infrastructure is not a completed capability until a real consumer path and acceptance
evidence exist.

## Numbering

Unlike i2pr, this fresh repository follows CodeGG's default: milestone numbers are local
to a subsystem roadmap. The initial `sam-library` workstream uses `001`, `002`, etc.
Correctives receive new local numbers; completed records are not rewritten to appear
successful retroactively.

## Lifecycle

1. Check canonical requirements and applicable ADRs.
2. Update/create the subsystem roadmap if architecture or sequencing changed.
3. Select one dependency-ready milestone.
4. Write a bounded plan under `implementation/<subsystem>/`.
5. Register it in `registry.md`.
6. Implement and execute the required evidence.
7. Write `closure/<subsystem>/NNN-status.md`.
8. Update registry/roadmap status and unblock only satisfied successors.
