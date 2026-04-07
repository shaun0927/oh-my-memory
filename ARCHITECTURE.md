# ARCHITECTURE — oh-my-memory

## 1. 설계 원칙

`oh-my-memory`의 아키텍처는 다음 네 가지를 우선합니다.

1. **저오버헤드**  
   daemon 자체가 시스템을 무겁게 만들면 안 된다.
2. **사용자 경험 보호**  
   현재 작업 중인 프로세스는 기본적으로 보호한다.
3. **설명 가능한 자동화**  
   왜 어떤 프로세스가 stale 후보가 되었는지 남겨야 한다.
4. **LLM 최소 사용**  
   LLM은 제어 plane이 아니라 advisory plane이다.

---

## 2. 전체 구조

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

---

## 3. 모듈 책임

## `telemetry`
- 시스템 메모리/스왑/프로세스 스냅샷 수집
- 상위 메모리 사용 프로세스 추출
- cheap path 중심
- process age / parent / stale enrichment 기초 데이터 제공

## `fingerprint`
- process family 추론
- playwright / browser automation / agent / tmux / watcher / build tool 분류

## `stale`
- parent missing / duplicate family / low CPU / long runtime 기반 stale score 계산
- cleanup 후보 / aggressive 후보 산출

## `models`
- `MemorySnapshot`
- `ProcessSample`
- `PressureLevel`
- `Decision`
- `ActionPlan`

## `config`
- TOML 설정 로드
- threshold, hooks, profiles, LLM budget 정의

## `policy`
- pressure level 판정
- LLM 호출 게이트 계산
- stale score 기반 action 우선순위 생성

## `actions`
- dry-run 또는 external hook 실행
- graceful/safe-first action execution report 생성

## `journal`
- latest snapshot 저장
- JSONL journal append
- 나중에 incident replay 가능하게 함

## `llm`
- compact prompt 생성
- 외부 LLM analyzer command 호출
- optional path

## `daemon`
- 주기적 루프 orchestration
- snapshot → policy → action → journal 순서 실행

## `cli`
- sample / daemon / explain / print-config 명령 정의

---

## 4. Hot Path vs Cold Path

## Hot Path
항상 자주 실행되는 경로:
- memory sampling
- top process sampling
- threshold evaluation
- action planning

특징:
- cheap
- deterministic
- no network dependency
- low CPU

## Cold Path
조건을 만족할 때만 실행:
- sustained pressure deeper analysis
- external cleanup hooks
- optional LLM analysis

특징:
- infrequent
- gated
- bounded by budget/cooldown

---

## 5. stale-first 메모리 관리

이 레포는 특정 도구를 먼저 이해하려고 하지 않습니다.
먼저 generic process 관점에서 아래를 봅니다.

- 메모리 크기
- CPU inactivity
- 생성 시각
- parent 존재 여부
- 반복/중복 프로세스 여부
- recent activity 여부
- protected 여부

즉:

> “이게 무거운가?”보다 “이걸 지금 정리해도 될 가능성이 높은가?”를 본다.

---

## 6. 보호 계층

정리 금지 대상의 예:
- foreground browser
- 최근 입력/활동이 있었던 세션
- main app process
- protected profile 매칭 대상

현재 MVP는 profile 기반 보호만 갖고 있으며,
앞으로는 다음이 추가될 수 있습니다.

- foreground app introspection
- tmux active pane inference
- recent activity heuristics
- parent-child tree confidence

---

## 7. action 철학

기본 순서:

1. observe only
2. recommendation
3. external cleanup hook
4. graceful terminate
5. destructive terminate (명시적 enable 필요)

현재 MVP는 1~3단계 중심입니다.

---

## 8. LLM 위치

LLM은 hot path 밖에 있어야 합니다.

호출 조건 예:
- pressure >= orange
- sustained interval threshold 충족
- cooldown 경과
- daily budget 남음

LLM의 역할:
- root cause 설명
- candidate action ordering 설명
- user-facing explanation 생성

LLM의 비역할:
- direct kill decision
- unlimited policy override
- continuous monitoring

---

## 9. 향후 진화 방향

### v0.2
- stale score / duplicate / orphan heuristic 반영
- process family inference 반영
- safe-first action ranking 강화

### v0.3
- tmux active pane protection
- browser automation stale detection 강화
- signal ladder 세분화

### v0.4+
- optional OpenChrome integration
- optional Codex/Claude metadata integration
- SQLite state backend
- dashboard

---

## 10. 결론

`oh-my-memory`의 아키텍처는
**connector-heavy orchestrator**가 아니라

> **low-overhead process-first stale cleanup daemon**

으로 설계됩니다.

이 구조가 가장 먼저 실제 문제를 줄이고,
가장 적은 리소스로,
가장 적은 사용자 경험 훼손으로,
메모리 압박을 관리할 수 있는 방식입니다.
