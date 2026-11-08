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
];

export const PROHIBITED_PATTERNS = PROHIBITED_CONTENT_B64.map(
  (b64) => new RegExp(Buffer.from(b64, "base64").toString("utf8"), "i")
);
