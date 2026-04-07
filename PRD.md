# PRD — oh-my-memory

## 1. 제품 정의

### 제품명
**oh-my-memory**

### 제품 설명
로컬 개발 환경에서 다수의 LLM 세션, 브라우저 자동화, tmux pane, helper 프로세스가 동시에 실행되는 상황에서,
**메모리 압박이 일정 수준을 넘으면 stale하고 안전하게 정리 가능한 프로세스를 자동으로 식별하고, 사용자 경험을 해치지 않는 방식으로 메모리를 관리하는 Rust 기반 personal memory janitor**.

이 제품의 핵심은 “메모리를 많이 먹는 걸 무작정 죽이는 것”이 아니라,
**사용 중인 작업은 보호하면서 stale한 무거운 프로세스를 먼저 정리하는 것**이다.

---

## 2. 문제 정의

사용자는 다음 환경을 동시에 운영할 수 있다.

- oh-my-codex / oh-my-claudecode / Codex / Claude 세션 다수
- headless Chrome / Playwright / OpenChrome worker 누적
- tmux pane, logs, build runner, watcher, MCP helper 중첩
- parent는 끝났지만 child/helper만 남은 orphan process

이때 시스템 메모리 문제는 단순히 “한 프로세스가 무겁다”가 아니라,
**무거운 stale 프로세스가 장시간 누적되는 것**에서 발생한다.

기존 접근의 문제:
1. 수치만 보여주고 아무 행동도 안 함
2. 위험한 프로세스 구분 없이 kill 함
3. LLM을 항상 호출해 도구 자체가 무거워짐

---

## 3. 제품 목표

### Functional goals
1. 시스템 메모리/스왑/프로세스 상태를 저비용으로 감시한다.
2. stale 가능성이 높은 무거운 프로세스를 식별한다.
3. foreground/recent/protected 작업은 자동 정리에서 제외한다.
4. pressure level에 따라 가장 안전한 액션을 자동 계획한다.
5. 필요할 때만 최소한으로 LLM을 사용한다.
6. 모든 판단과 액션을 설명 가능하게 기록한다.

### Non-goals
- 모든 도구에 대한 완전한 connector 우선 구축
- foreground 프로세스 aggressive kill
- 복잡한 ML 기반 예측 시스템
- GUI/dashboard 의무화

---

## 4. 핵심 제품 철학

## 4.1 Process-first
초기 버전은 connector-first가 아니라 **process-first** 다.

이유:
- 사용자가 원하는 immediate value는
  - stale Playwright 정리
  - orphan headless browser 정리
  - idle helper 정리
  - 오래된 watcher/test runner 정리
  이기 때문
- 이 대부분은 OS process 정보만으로도 상당 부분 해결 가능하다.

즉 v1의 중심은:
- process observer
- fingerprinting
- stale detector
- safety guard
- action planner
이다.

## 4.2 Safe-first
정리 순서는 항상 safe-first 이어야 한다.

순서:
1. 관찰만
2. 추천
3. low-risk cleanup
4. graceful terminate
5. destructive terminate (opt-in only)

## 4.3 LLM-minimal
LLM은 제어기가 아니라 advisor다.

- hot path에서 사용 금지
- sustained pressure + cooldown + budget 충족 시에만 호출
- compact summary만 전달

---

## 5. 왜 Rust인가

### 제품 요구사항
- 백그라운드 daemon
- 낮은 메모리 점유
- 낮은 CPU 오버헤드
- long-running 안정성
- 시스템 프로세스 다루기 적합
- 배포 간결성

### Rust 선택 이유
- 단일 바이너리
- predictable memory behavior
- 낮은 런타임 오버헤드
- 시스템 툴 성격에 적합
- deterministic control plane 구현에 유리

### Rust 구현 원칙
- 기본 daemon loop는 sync + sleep 기반
- async runtime는 필수 아닐 때 넣지 않음
- expensive analysis는 lazy evaluation
- 외부 툴 연동은 subprocess/HTTP bridge로 제한

---

## 6. 시스템 아키텍처

## 6.1 Layer 1 — Process Observer
역할:
- 메모리/스왑 수집
- 프로세스 테이블 수집
- snapshot 생성

수집 데이터:
- pid / ppid
- process name
- command line
- RSS / memory bytes
- CPU%
- process age
- system total/used/available memory
- total/used swap

목표:
- 싸고 빠르게 현재 상태를 읽는다.

## 6.2 Layer 2 — Fingerprinter
역할:
- connector 없이 process type을 대략 분류

예시 분류:
- playwright
- headless_browser
- browser_main
- codex_like
- claude_like
- tmux
- watcher
- build_runner
- helper
- unknown

입력:
- name
- command line
- parent chain 일부

출력:
- type
- confidence
- matched rule

## 6.3 Layer 3 — Stale Detector
역할:
- “무겁다”가 아니라 “지금 정리해도 안전할 가능성이 높은가”를 추정

주요 stale 신호:
- CPU 거의 0%가 N분 지속
- 메모리 큰데 변화 없음
- parent 종료됨
- 생성된 지 오래됨
- 중복된 sibling process 다수
- foreground 아님
- recent activity 없음

출력:
- stale score
- stale reasons
- cleanup confidence

## 6.4 Layer 4 — Safety Guard
역할:
- 자동 정리 금지 대상을 제외

보호 규칙:
- foreground process tree
- recent activity process
- protected patterns
- main browser app
- interactive shell foreground job

출력:
- protected / not_protected
- protection reasons

## 6.4.1 Runtime protection heuristics
v0.3부터 다음이 반영된다.

- recent CPU activity protection
- startup grace protection
- parent-chain inherited protection
- browser-main family protection

즉 단순 profile 보호를 넘어서,
**지금 막 실행되었거나 실제로 움직이고 있거나 active process의 부모 체인에 있는 작업**은
stale cleanup 후보에서 강하게 제외된다.

## 6.5 Layer 5 — Policy Engine
역할:
- 메모리 pressure level 계산
- intervention 필요 여부 결정

pressure levels:
- Green
- Yellow
- Orange
- Red
- Critical

입력:
- memory usage percent
- swap usage
- pressure duration
- stale candidate count
- protected vs unprotected ratio

출력:
- level
- reasons
- whether to invoke LLM
- max action budget for this cycle

## 6.6 Layer 6 — Action Planner
역할:
- pressure + stale candidates + safety rules를 바탕으로 action plan 생성

action types:
- noop
- recommend_only
- cleanup_hook
- graceful_terminate
- hard_terminate (opt-in)

planner rules:
- safest candidate first
- protected candidate 제외
- too-recent candidate 제외
- same family duplicate면 low-risk priority 상승

## 6.7 Layer 7 — Executor
역할:
- dry-run이면 계획만 출력
- 실제 실행 시 hook / signal 처리
- 결과 journaling

MVP 기본값:
- `dry_run = true`
- `execute_hooks = false`
- `allow_destructive = false`

## 6.8 Layer 8 — Optional LLM Advisor
역할:
- root cause 요약
- action ordering 설명
- user-facing explanation 생성

호출 조건:
- level >= configured minimum
- sustained intervals reached
- cooldown passed
- daily budget available

LLM 입력 최소화:
- memory summary
- swap summary
- top offenders N개
- stale candidates summary
- action candidates

LLM 출력:
- concise explanation
- safe action recommendation

---

## 6.9 Optional context providers

v0.4부터는 core daemon 위에 optional provider를 얹을 수 있어야 한다.

원칙:
- provider는 **있으면 사용**, 없으면 무시
- provider는 **판단기**가 아니라 **hint 공급자**
- core policy/action planner가 최종 결정을 유지

예:
- tmux provider → active pane PID 보호 힌트
- OpenChrome provider → protected/stale PID JSON 힌트

운영 원칙:
- 항상 호출하지 않는다
- pressure가 configured minimum level 이상일 때만 lazy query
- provider 실패는 daemon 전체 실패가 되면 안 된다

---

## 7. 리소스를 적게 먹게 만드는 설계

## 7.1 Cheap path / expensive path 분리
### Cheap path
항상 수행:
- 메모리/스왑 확인
- 상위 N개 process 수집
- 간단한 fingerprint
- threshold 판단

### Expensive path
Orange 이상일 때만:
- deeper stale scoring
- external hook proposal
- LLM analysis

## 7.2 Sampling strategy
기본 interval:
- 10~15초

pressure 심화 시:
- 더 짧아질 수 있으나 제한적

pressure 안정 시:
- interval 유지

## 7.3 No hot-path async sprawl
- 기본 loop는 blocking sleep
- 무거운 병렬/비동기 orchestration은 MVP에서 지양
- 단순하고 예측 가능하게 유지

## 7.4 Bounded memory footprint
- 전체 process history를 다 저장하지 않음
- latest snapshot + compact journal 유지
- top N + delta만 기록
- optional SQLite는 이후 버전

---

## 8. stale 판단 로직

## 8.1 Example stale signals
- `rss_mb > threshold`
- `cpu_percent < idle_threshold` for several intervals
- `parent_missing = true`
- `age_secs > min_age`
- `duplicate_family_count >= N`
- `recent_activity = false`
- `protected = false`

## 8.2 Example stale score
예시 점수 체계:
- +30 if memory very high
- +20 if CPU nearly idle for long enough
- +25 if parent missing
- +15 if process old
- +10 if duplicated siblings
- -50 if protected
- -40 if recent activity

결론:
- score >= 60 → cleanup candidate
- score >= 85 + orphan → aggressive candidate (if policy allows)

---

## 9. LLM 사용 정책

## 9.1 LLM이 꼭 필요한 경우
- sustained pressure가 계속되는데
- 후보가 많고
- 어떤 액션 순서가 UX에 제일 안전한지 설명이 필요할 때

## 9.2 LLM이 필요 없는 경우
- obvious stale playwright process
- obvious orphan helper
- simple low-risk cleanup candidate 존재

## 9.3 호출 budget 정책
- 일일 최대 호출 횟수
- level 기반 최소 호출 조건
- cooldown
- dry-run explain mode 우선

---

## 10. connector 전략

### v1
- connector optional
- process-first 구조로 충분한 가치 제공

### v2+
정확도 향상을 위해 선택적 connector 추가 가능:
- OpenChrome integration
- tmux activity resolver
- codex/claude session metadata

중요:
connector는 v1의 본질이 아니라 **정확도 향상 수단**이다.

---

## 11. CLI 기능

### `sample`
한 번 측정하고
- snapshot
- pressure level
- top offenders
- planned actions
출력

### `daemon`
지속 감시
- sample
- evaluate
- maybe plan
- maybe execute
- journal

### `explain`
latest snapshot 기반으로
- compact LLM prompt 출력
- 또는 external analyzer 실행

### `print-config`
기본 설정 출력

---

## 12. 성공 기준

### MVP 성공 기준
- build 성공
- tests 성공
- single sample 실행 가능
- daemon loop 실행 가능
- policy decision 생성 가능
- action plan 생성 가능
- journal 기록 가능
- README/PRD가 명확함

### 제품 성공 기준
- 평상시 daemon 오버헤드 작음
- stale heavy processes를 안전하게 식별
- foreground/recent work 보호
- 필요 시만 LLM 사용
- 사용자에게 “왜 이 정리를 했는지” 설명 가능

---

## 13. 출시 이후 우선순위

### v0.2
- stale score refinement
- recent activity heuristics
- signal ladder 개선
- better family/process-tree detection

### v0.3
- recent activity heuristics
- startup grace / parent-chain protection
- browser-main runtime protection
- safer terminate ladder

### v0.4
- optional tmux provider
- optional OpenChrome provider
- lazy context hint merge

### v0.5+
- optional Codex/Claude metadata integration
- dashboard
- checkpoint/suspend integrations
- adaptive policy tuning

---

## 14. 최종 설계 결론

`oh-my-memory`는 다음 구조를 채택한다.

- **Rust daemon core**
- **process-first observer**
- **stale-detector 중심 판단**
- **safe-first action planner**
- **optional LLM advisor**
- **connector는 후속 정확도 향상 수단**

즉, 이 프로젝트는 본질적으로

> **“메모리 압박 시 stale하고 안전한 무거운 프로세스를 알아서 정리해주는 저오버헤드 개인 메모리 관리 에이전트”**

를 구현하는 것이다.
