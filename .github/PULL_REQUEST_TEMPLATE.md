## 관련 이슈 (Related Issue)

<!-- 종료 대상은 `Closes #N`, 상위 에픽/백로그 참조는 `Refs #N`, 경미한 유지보수(C등급)는 `이슈 없음(C등급)`으로 작성합니다. -->
- Closes #
- Refs:

## 요약 (Summary)

- **배경 / 문제 (Problem)**:
- **해결 결과 (Outcome)**:

## 주요 변경 사항 (Changes)

- 
- 

## 작업 범위 (Scope)

### 포함 범위 (Included)
- 

### 제외 범위 및 후속 체크리스트 안내 (Excluded)
- 

## 결정론적 검증 및 증거 (Verification & Evidence)

| 검증 항목 | 실행 명령 / 확인 수단 | 결과 |
| :--- | :--- | :--- |
| 저장소 위생 가드 | `node scripts/check-repo-hygiene.mjs` | Pass |
| macOS 빌드 | `swift build` | Pass |
| 단위/통합 테스트 | `swift test` | Pass |
| UI / 릴리스 증거 | 해당 시 스크린샷·번들 검증 로그 또는 불필요 사유 명시 | |

### 미실행 항목 (Not Run)
- 없음 (해당 시 구체적 사유 기재)

## 위험도 및 롤백 (Risk & Delivery)

- **위험 등급**: Class A / Class B / Class C
- **주요 리스크 및 Fail-Closed 동작**:
- **롤백 계획**: 이 PR을 Revert하여 즉시 복구 가능

## 체크리스트 (Checklist)

- [ ] 마이크로 PR로 쪼개지 않고 연관 변경(구현 + RED/GREEN 테스트 + 문서/정책)을 논리적 1단위로 묶었다.
- [ ] `node scripts/check-repo-hygiene.mjs`, `swift build`, `swift test`가 로컬에서 모두 통과했다.
- [ ] 코드·주석·커밋·PR 본문에 외부 상용 서비스명이나 불필요한 메타데이터를 남기지 않았다.
- [ ] 정책이나 기본값을 변경한 경우 `docs/policy-decisions.md`를 같은 PR에서 갱신했다.
- [ ] 코드리뷰 및 보안 검토 지적 사항을 모두 반영하고 모든 리뷰 스레드를 해결(`Resolved`)했다.
