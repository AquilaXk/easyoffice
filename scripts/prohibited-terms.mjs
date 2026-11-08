// Prohibited content shared by the repository hygiene guard (tracked files) and the commit message
// guard (PR commits, title and body). Kept Base64-encoded so this file does not itself contain the
// terms it rejects.

// Prohibited external brand tokens + tool/agent tokens in tracked files (Base64 encoded)
export const PROHIBITED_CONTENT_B64 = [
  "cG9sYXJpcyBvZmZpY2U=",     // external brand (en)
  "cG9sYXJpc29mZmljZQ==",     // external brand compact
  "7Y+0652866as7Iqk",         // external brand (ko)
  "7ZWc7Lu07Jik7ZS87Iqk",     // external brand 2 (ko)
  "aGFuY29tIG9mZmljZQ==",     // external brand 2 (en)
  "QUdFTlRTLm1k",             // local-only doc ref 1
  "Q0xBVURFLm1k",             // local-only doc ref 2
  "R0VNSU5JLm1k",             // local-only doc ref 3
  "QW50aWdyYXZpdHk=",         // tool name 1
  "Q2xhdWRlIENvZGU=",         // tool name 2
  "YXF1aWxhLXJldmlldw==",     // internal review skill name
  "c3ViYWdlbnQ=",             // internal orchestration term
  "Q28tQXV0aG9yZWQtQnk6",     // attribution header
  "Q2xhdWRlLVNlc3Npb246",     // session attribution trailer
  "XGJob3BcYg==",           // reference app name (word)
  "Z29sYmlu",             // reference app owner
  "7ZWc6riA6rO87Lu07ZOo7YSw", // external company name (ko); only the required spec notice may carry it, see NOTICE_EXCEPTIONS
];

export const PROHIBITED_PATTERNS = PROHIBITED_CONTENT_B64.map(
  (b64) => new RegExp(Buffer.from(b64, "base64").toString("utf8"), "i")
);

const decode = (b64) => Buffer.from(b64, "base64").toString("utf8");

// PD-122: the HWP public specification requires its copyright notice (the sentence is shown in the
// About dialog exactly as upstream words it). It names the company, so the sentence is the only text
// the hygiene guard lets carry that name, and only in the files listed here. Any other occurrence of
// the name, in these files or elsewhere, is still reported. Commit messages get no exception.
export const NOTICE_EXCEPTIONS = [
  {
    // "본 제품은 …의 한글 문서 파일(.hwp) 공개 문서를 참고하여 개발하였습니다." (rhwp-studio ko.ts)
    textB64: "67O4IOygnO2SiOydgCDtlZzquIDqs7zsu7Ttk6jthLDsnZgg7ZWc6riAIOusuOyEnCDtjIzsnbwoLmh3cCkg6rO16rCcIOusuOyEnOulvCDssLjqs6DtlZjsl6wg6rCc67Cc7ZWY7JiA7Iq164uI64ukLg==",
    files: ["crates/easyoffice-app/ui/index.html"],
  },
];

/** The exact notice sentences that `rel` (root-relative, forward slashes) may carry. */
export function allowedNotices(rel) {
  return NOTICE_EXCEPTIONS.filter((entry) => entry.files.includes(rel)).map((entry) => decode(entry.textB64));
}

/** Patterns that match `content` of the tracked file `rel` once its allowed notice sentences are set aside. */
export function prohibitedPatternsIn(rel, content) {
  let rest = content;
  for (const notice of allowedNotices(rel)) rest = rest.split(notice).join("");
  return PROHIBITED_PATTERNS.filter((pattern) => pattern.test(rest));
}
