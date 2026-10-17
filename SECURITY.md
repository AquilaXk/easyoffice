# Security Policy

## Supported Versions

Security updates are applied to the latest release on the `main` branch and published via official GitHub Releases.

| Version | Supported |
| :--- | :--- |
| `0.1.x` (latest `main`) | Yes |

## Security Architecture & Invariants

EasyOffice enforces strict **Fail-Closed** security invariants across all document parsers, formula evaluators, and format converters:

1. **Container & Decompression Bomb Defense**:
   - ZIP-based containers (`DOCX`, `XLSX`, `PPTX`, `HWPX`, `ODT`, `ODS`, `ODP`) and OLE2 Compound File Binary containers (`HWP 5.0`, `DOC`, `XLS`, `PPT`) enforce strict entry count, single-stream byte size, total uncompressed byte size, and compression ratio ceilings before allocation.
2. **Path Traversal & External Entity Blocking**:
   - Archive entry paths containing `..`, leading `/`, or symbolic links escaping the ephemeral workspace are rejected immediately.
   - XML parsers disable external DTDs and external general/parameter entities (XXE defense).
3. **Magic-Byte Signature Verification**:
   - File ingestion verifies binary magic signatures (`PK\x03\x04`, `\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1`, `%PDF-`, UTF-8 text validation) rather than trusting file extensions alone.
4. **Zero Silent Fallbacks**:
   - Corrupted headers, unsupported encryption schemes, or malformed streams return explicit typed errors (`EasyOfficeError`) without partial or fabricated output.

## Reporting a Vulnerability

Please report suspected vulnerabilities privately via GitHub Security Advisories on the repository rather than opening a public issue containing exploit payloads.
