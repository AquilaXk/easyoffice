# hayro-write 패치 (PD-266)

- 출처: crates.io `hayro-write` 0.9.0 (https://github.com/LaurenzV/hayro, `crates/hayro-write`), Apache-2.0 OR MIT,
  Copyright (c) The Hayro Authors. `src/`, `LICENSE-APACHE`, `LICENSE-MIT`, `README.md`와 `Cargo.toml`(예제 항목만 뺌)을
  그대로 복사했다.
- 바꾼 곳 1, `src/primitive.rs`: 복사할 때 버리는 키 목록(`IGNORE_KEYS`)에서 `/OC`를 뺐다. 쪽에서 복사하는 그림·양식·
  주석 외관 객체가 자기 선택적 내용(레이어) 참조를 잃으면, 숨긴 레이어의 내용이 저장 뒤 보인다
  (ISO 32000-1 §8.11.3, 8.11.4.2). `/Metadata`·`/AF`·`/PieceInfo` 등 나머지 키는 그대로 버린다.
- 바꾼 곳 2, `src/lib.rs`: `ExtractionQuery::new_object(ObjRef)`와 `ExtractionError::InvalidObject`를 더했다. 쪽과 같은
  호출 안에서 간접 객체 하나와 그것이 가리키는 객체를 복사하고, 쪽 안의 같은 객체와 같은 새 참조를 돌려준다. 호출부가 레이어
  목록·첨부 파일·주석 외관을 쪽의 자원과 같은 객체로 잇는 데 쓴다.
- 원본 684줄(`src/lib.rs` 462줄, `src/primitive.rs` 222줄) 가운데 3줄을 바꿨고(`use` 줄 재배열 2줄, `m.insert(OC)` 1줄),
  머리 주석 6줄을 포함해 39줄을 더했다. 버전을 올릴 때는 이 두 곳만 다시 적용한다.
