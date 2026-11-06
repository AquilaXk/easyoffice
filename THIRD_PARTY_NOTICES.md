# Third-Party Notices

EasyOffice's own code is licensed under the repository LICENSE. This file lists third-party
material that is pinned, vendored or bundled, with the notices their licenses require.

## rhwp (upstream submodule, MIT)

Source: https://github.com/edwardkim/rhwp (pinned at the commit recorded in `config/rhwp-upstream.json`, version 0.8.7).
It is consumed unmodified as the read-only submodule `third_party/rhwp` (PD-076).

```
MIT License

Copyright (c) 2025-2026 Edward Kim

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Rust dependencies compiled into the rhwp WebAssembly module

Placeholder, to be completed in phase 2 (when the WebAssembly module is vendored). It will list
every crate statically linked into the module with its license text (MIT, Apache-2.0, BSD,
Zlib, ISC, 0BSD), generated from the upstream `THIRD_PARTY_LICENSES.md` and re-verified offline
at each upstream update. Apache-2.0 crates that ship a `NOTICE` file will have that file
reproduced here verbatim.

Nothing from this section is shipped in a release yet.

## Fonts

Placeholder, to be completed in phase 2. Only fonts under the SIL Open Font License 1.1 (and
other freely redistributable licenses) taken unmodified from upstream will be bundled, each with
its license text and source. Proprietary and operating-system fonts are never bundled.

Nothing from this section is shipped in a release yet.

## hop (patterns only)

hop (MIT) is a reference for build and verification patterns. Its design patterns were followed
and no code has been copied. If code is copied later, its MIT notice must be added here first.
