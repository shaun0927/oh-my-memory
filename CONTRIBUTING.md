# CONTRIBUTING

## 개발 원칙

- hot path는 싸게 유지합니다.
- LLM은 optional path로만 추가합니다.
- 기본 동작은 safe-first / dry-run 중심이어야 합니다.
- foreground/active workload를 자동 terminate 하는 코드는 매우 신중하게 다룹니다.

## 개발 루프

```bash
cargo fmt --all --check
cargo check
cargo test
```

## 문서 변경 시

다음 문서를 함께 갱신해야 합니다.

- `README.md`
- `PRD.md`
- `ARCHITECTURE.md`
- `ROADMAP.md`

## Scope rule

tool-specific integration을 넣기 전에 먼저 확인할 것:

1. generic process-first 방식으로 해결 가능한가?
2. integration이 hot path를 무겁게 만들지 않는가?
3. integration 없이도 graceful degradation이 가능한가?
