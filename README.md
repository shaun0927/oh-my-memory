# oh-my-memory

> **나만의 메모리 관리 비서**  
> 병렬로 많이 띄운 LLM 세션, 브라우저 자동화, tmux 작업, MCP/CLI helper들 때문에 시스템 메모리가 압박받을 때,  
> **현재 작업을 최대한 보호하면서 stale하고 무거운 프로세스를 먼저 정리해 주는 Rust 기반 메모리 관리 에이전트**.

---

## 이 레포는 무엇을 만들기 위한 것인가

`oh-my-memory`는 단순한 메모리 모니터가 아닙니다.
또한 “메모리 많이 먹는 프로세스를 큰 순서대로 죽이는 킬 스크립트”도 아닙니다.

이 레포가 만드는 것은 다음과 같은 성격의 도구입니다.

- **항상 백그라운드에서 아주 가볍게** 시스템 메모리와 프로세스 상태를 본다.
- 메모리 압박이 커지면,
- **지금 사용 중인 작업은 보호한 채**,
- **오래 방치된(stale) 무거운 프로세스**를 우선 정리한다.
- 필요할 때만 LLM에게 “왜 이런 상황이 생겼는지” 또는 “이번엔 어떤 순서가 가장 안전한지”를 설명하게 한다.

즉, 이 프로젝트의 본질은 다음입니다.

> **generic process observer + stale detector + safe cleanup agent**

핵심은 “특정 도구용 connector를 많이 만드는 것”이 아니라,
**프로세스를 generic하게 관찰하고, stale 여부를 판단하고, 안전하게 정리하는 것**입니다.

---

## 왜 이런 게 필요한가

개발 환경이 무거워지는 이유는 생각보다 단순하지 않습니다.

예를 들면:
- `oh-my-codex`, `oh-my-claudecode`, Codex/Claude CLI 세션을 여러 개 병렬 실행
- OpenChrome, Playwright, headless Chrome/Chromium 프로세스가 누적
- tmux pane, shell job, watcher, tail, MCP helper가 계속 살아 있음
- parent는 끝났는데 child/helper만 orphan 상태로 남아 있음
- 브라우저 탭은 안 보고 있는데 renderer/helper는 계속 메모리를 차지함

이 상황에서 보통 두 가지 실패가 생깁니다.

### 1) 관찰만 하고 끝나는 도구
“메모리 89% 사용 중” 같은 숫자만 보여 줍니다.
하지만 사용자는 여전히 모릅니다.

- 뭐가 진짜 원인인지
- 뭘 정리해도 안전한지
- 지금 손대면 안 되는 프로세스가 뭔지

### 2) 지나치게 공격적인 자동 정리
무턱대고 kill 하면,
정리 대상이 stale browser worker가 아니라 **내가 막 작업하던 세션**일 수도 있습니다.

이 프로젝트는 이 두 극단 사이를 목표로 합니다.

---

## 이 레포의 핵심 철학

## 1. 사용자 경험 보호 우선
메모리를 낮추는 것보다 먼저 중요한 것은:

- 지금 보고 있는 창/작업을 안 깨는 것
- 최근에 입력하던 세션을 보호하는 것
- 복구 불가능한 손상을 일으키지 않는 것

즉,
**foreground / recent / protected workload는 기본적으로 건드리지 않는다**가 원칙입니다.

## 2. stale 먼저 정리
정리 우선순위는 “메모리 큰 순서”가 아닙니다.
정확한 우선순위는 대략 다음과 같습니다.

1. stale helper
2. orphan child process
3. idle browser automation
4. 오래된 watcher/tail/test runner
5. background low-priority workload
6. 마지막 수단으로 destructive action

즉,
**biggest-first가 아니라 safest-first** 입니다.

## 3. LLM은 옵션이고, 메인은 규칙 엔진
이 프로젝트의 제어 plane은 Rust입니다.
LLM은 다음 역할만 맡습니다.

- root cause 설명
- 사람이 읽기 쉬운 요약
- 애매한 경우의 우선순위 자문

즉,
- **daemon이 판단한다**
- **LLM은 설명/조언한다**

---

## 왜 Rust인가

`oh-my-memory`는 백그라운드 daemon 성격이 강합니다.
즉, 도구 자체가 무거우면 실패입니다.

Rust를 택한 이유:
- 낮은 메모리 오버헤드
- 낮은 CPU 사용량
- long-running process 안정성
- 단일 바이너리 배포 가능
- 시스템 메트릭 수집 및 프로세스 다루기에 적합
- 제어 로직을 deterministic하게 유지하기 좋음

이 프로젝트는 “LLM 앱”보다
**경량 시스템 도구**에 가까우므로 Rust가 매우 잘 맞습니다.

---

## 아키텍처 개요

### Layer 1 — Process Observer
가장 싸고 자주 도는 계층입니다.

수집 항목:
- total / used / available memory
- swap usage
- top N processes
- pid / ppid / name / command line
- RSS / memory
- CPU%
- process age
- snapshot delta

여기서는 아직 아무 것도 정리하지 않습니다.
그냥 **현재 상황을 싸게 읽어옵니다.**

### Layer 2 — Fingerprinter
특정 tool connector 없이도,
프로세스 이름과 command pattern으로 대략적인 유형을 분류합니다.

예:
- `playwright`
- `chrome --headless`
- `codex`
- `claude`
- `tmux`
- `node`
- `python`
- `mcp`
- `watcher`

즉 처음부터 OpenChrome/tmux/codex connector를 강하게 붙이지 않아도,
**generic process pattern** 만으로 상당수의 무거운 stale workload를 잡을 수 있습니다.

### Layer 3 — Stale Detector
이 프로젝트의 핵심입니다.

프로세스를 stale 후보로 보는 신호 예:
- CPU가 매우 낮은 상태가 오래 지속됨
- 메모리는 큰데 최근 변화가 거의 없음
- parent가 이미 사라짐
- 생성된 지 오래됨
- 중복 프로세스가 많음
- foreground/protected 아님

즉,
**이게 무겁다** 가 아니라
**이게 지금 정리해도 안전할 가능성이 높다** 를 판단하는 층입니다.

### Layer 4 — Safety Guard
다음은 기본 보호 대상입니다.

- 현재 foreground app 관련 프로세스
- 최근 몇 분 내 활동이 있었던 프로세스
- 사용자가 protect로 지정한 프로세스
- 복구 불가능 가능성이 높은 메인 앱

예를 들어:
- `Google Chrome` 메인 앱은 기본 보호
- 대신 orphan/headless/browser automation child는 후보가 될 수 있음

### Layer 5 — Policy Engine
메모리 상태를 단계로 나눕니다.

- Green
- Yellow
- Orange
- Red
- Critical

그리고 각 단계에서 허용되는 행동을 제한합니다.

예:
- Green: 아무것도 안 함
- Yellow: 경고/추천
- Orange: 저위험 stale cleanup 후보 생성
- Red: soft terminate / suspend 류 검토
- Critical: 명시 허용된 강한 조치만

### Layer 6 — Action Engine
행동은 반드시 계단식으로 올라갑니다.

1. observe only
2. 추천
3. soft cleanup
4. graceful terminate
5. 강한 terminate (opt-in)

**기본은 dry-run** 입니다.

### Layer 7 — Optional LLM Advisor
LLM은 hot path에 없습니다.
다음 조건에서만 호출됩니다.

- pressure가 충분히 높고
- 몇 번 연속 지속되었고
- cooldown이 지났고
- daily budget이 남아 있을 때

그리고 LLM에게 보내는 데이터도 최소화합니다.
- memory summary
- swap summary
- top offenders
- stale candidate 목록
- planned actions

---

## 왜 connector-first가 아닌가

이 프로젝트에서 **진짜 중요한 것**은
OpenChrome/tmux/codex 전용 connector를 먼저 많이 붙이는 게 아닙니다.

당장 필요한 것은:
- Playwright 같은 무거운 stale 작업
- orphan headless browser
- 오래된 watcher/test runner
- 방치된 helper process

같은 것들을 **generic process layer에서 자동 정리하는 것**입니다.

즉 초기 버전은
**connector-first** 가 아니라
**process-first** 여야 합니다.

connector는 나중에 정확도를 높이는 수단입니다.
예를 들어:
- 이 Chrome process가 진짜 보고 있는 탭인지
- 이 tmux pane이 실제 active인지
- 이 codex agent가 checkpoint 가능한지

이런 걸 더 정확히 알고 싶을 때 붙입니다.

하지만 v1 목표 달성에는 필수가 아닙니다.

---

## LLM을 어떻게 최소한으로 쓸 것인가

### 기본 원칙
- Green / Yellow에서는 호출 안 함
- Orange 이상이어도 즉시 호출 안 함
- sustained pressure + cooldown + budget 조건을 만족해야 함

### LLM이 하는 일
- 원인 설명
- 우선순위 설명
- 사용자에게 보여줄 concise summary 생성

### LLM이 하지 않는 일
- 무조건적인 kill decision
- unrestricted action execution
- hot path monitoring

즉,
**LLM은 메모리 관리자 그 자체가 아니라 설명 가능한 분석기** 입니다.

---

## 이 레포의 현재 MVP 범위

현재 MVP는 다음을 제공합니다.

- 시스템 메모리/프로세스 스냅샷 수집
- pressure level 계산
- process profile 기반 중요도 분류
- safe-first action plan 생성
- dry-run execution path
- JSONL journal / latest snapshot 저장
- compact LLM prompt 생성 또는 optional external analyzer 실행

포함되지 않은 것:
- GUI dashboard
- OpenChrome/tmux/codex 정밀 connector
- aggressive automation
- full production hardening

즉,
지금은 **작동하는 기반 + 명확한 설계 문서 + 확장 가능한 구조** 입니다.

---

## 대시보드가 꼭 필요한가

아니요.
초기 버전에서는 **필수 아닙니다.**

이 프로젝트의 성공 조건은
UI가 아니라 다음입니다.

- daemon이 충분히 가벼운가
- stale 판단이 안전한가
- action이 explainable한가
- LLM 호출이 sparse한가

그래서 v1은
**CLI + config + journal** 중심이 맞습니다.

대시보드는 이후에 다음 용도로 확장 가능합니다.
- 추세 시각화
- action history review
- 보호 규칙 편집
- memory incident replay

하지만 초기 목적 달성에는 필요 없습니다.

---

## 사용 예시

### 한 번 스냅샷 보기
```bash
cargo run -- sample --top 12
```

### daemon 실행
```bash
cargo run -- daemon --config config/oh-my-memory.example.toml
```

### LLM 설명 프롬프트 생성
```bash
cargo run -- explain --config config/oh-my-memory.example.toml
```

### 기본 설정 출력
```bash
cargo run -- print-config
```

---

## 앞으로의 우선순위

### v0.2
- stale score 정교화
- orphan / duplicate / long-idle heuristics 고도화
- safe terminate ladder 개선

### v0.3
- foreground / recent activity 보호 강화
- tmux activity introspection
- browser automation process family 개선

### v0.4+
- optional OpenChrome integration
- optional tmux integration
- optional codex/claude session metadata integration
- dashboard

---

## 결론

`oh-my-memory`는
“특정 도구 connector를 많이 붙인 관리 대시보드”가 아니라,

> **현재 작업을 보호하면서 stale하고 무거운 프로세스를 우선 정리하는 저오버헤드 Rust 메모리 관리 에이전트**

로 설계되어야 합니다.

그리고 그 목표를 위해 이 레포는:
- process-first
- stale-detector-first
- deterministic policy-first
- safe-first action ladder
- optional LLM advisor
구조를 채택합니다.

이게 가장 현실적이고,
가장 리소스를 적게 먹고,
가장 사용자 경험을 덜 해치는 방식입니다.
