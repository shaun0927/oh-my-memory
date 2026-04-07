# oh-my-memory

[![Rust](https://img.shields.io/badge/Rust-1.93%2B-black?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![GitHub stars](https://img.shields.io/github/stars/shaun0927/oh-my-memory?style=social)](https://github.com/shaun0927/oh-my-memory)

**A personal memory management assistant for heavy local AI workflows.**

Don't babysit Activity Monitor.  
Let `oh-my-memory` watch memory pressure, detect stale heavy processes, protect your active work, and suggest the safest cleanup path.

**Get Started** • [PRD](./PRD.md) • [Architecture](./ARCHITECTURE.md) • [Roadmap](./ROADMAP.md) • [Contributing](./CONTRIBUTING.md)

* * *

## What is oh-my-memory?

`oh-my-memory` is a lightweight Rust daemon and CLI for developers who routinely run:

- many parallel Codex / Claude / agent sessions
- browser automation workloads like Playwright or headless Chromium
- tmux panes, shell jobs, watchers, and MCP helpers
- long-lived helper processes that quietly accumulate memory over time

It is designed to be your **personal memory janitor**:

- always on
- low overhead
- safe by default
- explainable
- selective about when it uses an LLM

This project does **not** aim to be a generic dashboard first.  
It aims to be a **practical memory-management agent** that helps you keep your machine responsive **without killing the thing you are actively using**.

---

## Why this exists

When local AI-heavy workflows get messy, memory problems rarely come from one obvious process.

Usually it looks more like this:

- a few stale Playwright runners are still alive
- several headless browser children survived after their parent exited
- old agent sessions are idle but still holding memory
- logs/watchers/helpers are still running in the background
- your browser is protecting useful tabs *and* wasteful background processes at the same time

The usual solutions are unsatisfying:

### Option 1 — Observe only
You get numbers, but no judgment.

- *What is actually stale?*
- *What is safe to clean up?*
- *What is dangerous to touch right now?*

### Option 2 — Kill first, think later
You reclaim memory, but maybe at the cost of:

- the pane you were actively using
- the browser tab you were reading
- a process that was slow, not stale

`oh-my-memory` sits between those extremes.

Its job is not to “kill the biggest thing.”  
Its job is to ask:

> **What is heavy, stale, and safe enough to clean up first?**

---

## Core philosophy

### 1. Protect active work first
Foreground and recently active workloads are more important than a perfect memory number.

If a process is likely tied to what you are doing right now, it should be protected by default.

### 2. Clean up stale processes first
The target is not “large process.”  
The target is “large, stale, low-risk process.”

Examples:

- orphaned headless browser children
- stale automation runners
- long-idle watchers
- duplicate helpers
- background support processes that can be recreated

### 3. The daemon must stay cheap
A memory management tool that consumes too many resources has already failed.

So `oh-my-memory` keeps the hot path intentionally simple:

- periodic lightweight sampling
- deterministic policy checks
- low-cost action planning

Heavy analysis is delayed until it is truly needed.

### 4. LLMs are optional advisors, not controllers
The Rust daemon is the control plane.

The LLM, when enabled, is only used for:

- explaining likely root causes
- ranking already-safe candidate actions
- generating a concise human-readable summary

The LLM is **not** responsible for:

- continuous monitoring
- unrestricted cleanup decisions
- overriding safety policy

---

## Design direction

`oh-my-memory` is intentionally **process-first**, not **connector-first**.

That means the first version focuses on:

- observing generic OS processes
- fingerprinting common heavy workload families
- detecting stale behavior
- ranking low-risk cleanup actions

instead of requiring deep integrations with every tool on day one.

This matters because the immediate value usually comes from generic cases like:

- stale Playwright jobs
- orphaned browser automation processes
- idle helper daemons
- duplicate background runners

You do **not** need a full OpenChrome or tmux connector to start solving those problems.

Connectors can come later as **accuracy upgrades**, not as the foundation.

---

## How it works

At a high level:

```text
sample / daemon / explain / print-config
                │
                ▼
         Process Observer
                │
                ▼
           Fingerprinter
                │
                ▼
          Stale Detector
                │
                ▼
           Safety Guard
                │
                ▼
           Policy Engine
                │
                ▼
           Action Planner
                │
        ┌───────┴────────┐
        ▼                ▼
     Executor       LLM Advisor
        │                │
        └───────┬────────┘
                ▼
        Journal / Latest State
```

### Process Observer
Cheap memory and process sampling:

- total / used / available memory
- swap usage
- top processes by memory
- pid / parent pid / command / age / cpu

### Fingerprinter
Assigns generic process families such as:

- browser main
- browser automation
- agent
- tmux / multiplexer
- watcher
- build tool
- helper

### Stale Detector
Scores processes using signals like:

- very low CPU over time
- high memory usage
- long runtime
- missing parent
- duplicate family count
- protection penalties

### Safety Guard
Prevents dangerous cleanup by default.

Protected categories include:

- foreground-like workloads
- recent activity
- explicitly protected profiles
- critical main application processes

### Policy Engine
Evaluates pressure levels:

- Green
- Yellow
- Orange
- Red
- Critical

and decides whether to:

- do nothing
- advise
- plan hooks
- gracefully terminate low-risk stale candidates
- consider stronger action only if explicitly allowed

### Optional LLM Advisor
Only invoked when:

- pressure is high enough
- the situation is sustained
- cooldown has passed
- daily budget allows it

And even then, it receives only compact context:

- memory summary
- swap summary
- top offenders
- stale candidates
- planned actions

---

## Why Rust?

Because this tool is supposed to live in the background without becoming part of the problem.

Rust is a strong fit for:

- low runtime overhead
- stable long-running daemons
- predictable memory behavior
- single-binary delivery
- efficient system/process inspection

`oh-my-memory` is much closer to a **systems utility** than a typical LLM application, so Rust is the right foundation.

---

## Current MVP

The current repository already includes a working foundation:

- memory/process snapshot collection
- pressure level evaluation
- process profile classification
- process family fingerprinting
- stale scoring based on runtime, CPU, duplication, orphan state, and protection
- safe-first action planning
- dry-run execution
- JSONL journaling and latest snapshot output
- compact LLM prompt generation and optional external analyzer support

What it does **not** include yet:

- deep tool-specific connectors
- GUI dashboard
- aggressive automation by default
- long-horizon behavioral learning

So today it is best understood as:

> **a working, low-overhead, explainable foundation for a real memory-management agent**

---

## Quick Start

### 1. Clone

```bash
git clone https://github.com/shaun0927/oh-my-memory.git
cd oh-my-memory
```

### 2. Build

```bash
cargo build
```

### 3. Print the default config

```bash
cargo run -- print-config
```

### 4. Take one snapshot

```bash
cargo run -- sample --top 12
```

### 5. Run the daemon

```bash
cargo run -- daemon --config config/oh-my-memory.example.toml
```

### 6. Generate an explanation prompt

```bash
cargo run -- explain --config config/oh-my-memory.example.toml
```

---

## Configuration model

The example config lives at:

```text
config/oh-my-memory.example.toml
```

It currently controls:

- sampling interval
- top process count
- memory/swap thresholds
- stale scoring thresholds
- LLM cooldown and budget
- dry-run behavior
- external cleanup hooks
- process profiles

By default:

- destructive actions are disabled
- hook execution is disabled
- LLM analysis is disabled

That means the default setup is intentionally safe.

---

## Safety model

`oh-my-memory` is built around **safe-first remediation**.

Typical response ladder:

1. observe only
2. recommend
3. run low-risk cleanup hook
4. graceful terminate candidate
5. hard terminate only if explicitly enabled

This repo is intentionally conservative.

If there is doubt, the daemon should prefer:

- logging
- explaining
- recommending

over taking destructive action.

---

## Roadmap

### v0.1 — Foundation
- public repo
- README / PRD / architecture docs
- Rust CLI + daemon
- snapshot collection
- pressure policy
- journaling

### v0.2 — Better stale detection
- process family fingerprinting
- stale score model
- duplicate/orphan heuristics
- safer action ranking

### v0.3 — Better protection
- recent activity heuristics
- stronger protection model
- better terminate ladder

### v0.4+
- optional tmux integration
- optional browser automation integration
- optional OpenChrome integration
- optional dashboard

See [ROADMAP.md](./ROADMAP.md) for the fuller plan.

---

## Documentation

- [PRD](./PRD.md)
- [Architecture](./ARCHITECTURE.md)
- [Roadmap](./ROADMAP.md)
- [Contributing](./CONTRIBUTING.md)

---

## Final takeaway

`oh-my-memory` is not trying to be a flashy dashboard first.

It is trying to become:

> **a low-overhead personal memory janitor that protects active work, detects stale heavy processes, and keeps your machine usable under AI-heavy local workloads**

That is the product.
Everything else—connectors, dashboards, richer orchestration—comes later.
