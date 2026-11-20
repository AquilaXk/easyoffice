# Third-Party Notices

EasyOffice's own code is licensed under the repository LICENSE. This file lists third-party
material that is pinned, vendored or bundled, with the notices their licenses require.

## rhwp (upstream submodule, MIT)

Source: https://github.com/edwardkim/rhwp (pinned at the commit recorded in `config/rhwp-upstream.json`, version 0.8.7).
It is consumed unmodified as the read-only submodule `third_party/rhwp` (PD-076).
The selection colour on paper, `rgba(51,144,255,0.35)` in `crates/easyoffice-app/ui/content.css`, is the overlay colour of `rhwp-studio/src/engine/selection-renderer.ts:94` (PD-267).
This notice also covers what is derived from it and shipped: the prebuilt WebAssembly package in `vendor/rhwp-core` (PD-081), the rhwp-studio editor sources that the `apps/hwp-studio` overlay bundles, the HWPX OWPML `hh:numberings`, `hh:bullets` and `hh:paraHead` multi-level list schema adapted into `crates/easyoffice-formats/src/rich_list_writers.rs` (PD-234, from `src/serializer/hwpx/header.rs` lines 448-460 and 792-958, rewritten against this repository's streaming writer and reader with a packed `u64` key, a 32-byte stack buffer and an 8,192-head budget), the HWPX OWPML drawing shapes (`hp:rect`, `hp:ellipse`, `hp:polygon`, `hp:curve`, `hp:arc`, `hp:line`, `hp:connectLine`, `hp:container`, `hp:textart`, `hp:drawText`, `hp:caption`), `hp:equation` scripts, table-cell `hp:pic` attachments, and `hp:footNote` / `hp:endNote` / `hp:footNotePr` / `hp:endNotePr` controls adapted into `crates/easyoffice-formats/src/rich_read_hwpx/objects.rs` and `frame.rs` (PD-247, from `src/parser/hwpx/section.rs` lines 1277-1545, 2692-2745, 4464-5050, 5642-5778, and 6835-6920, rewritten against this repository's `XmlReader` with a 16-level nesting cap, a 10,000-note cap, and a 64 KiB equation script cap), and the rhwp-studio code adapted into the EasyOffice WebUI (PD-098): `crates/easyoffice-app/ui/tokens.css` (from `rhwp-studio/src/styles/base.css`, values unchanged, icon-sprite tokens left out) and `crates/easyoffice-app/ui/theme.js` (from `rhwp-studio/src/core/theme.ts` and `public/theme-init.js`, rewritten as plain JavaScript with the same `data-theme-mode` / `data-theme-effective` attributes), `crates/easyoffice-app/ui/menu-bar.css` (from `rhwp-studio/src/styles/menu-bar.css`, same selectors and values) `crates/easyoffice-app/ui/toolbar.css` (surface, border, hover and separator values of `toolbar.css`, `style-bar.css` and `status-bar.css`) and the dialog row and note rules of `crates/easyoffice-app/ui/shell.css` (`.dialog-row`, `.dialog-label`, `.dialog-input`, `.dialog-pdf-note` from `rhwp-studio/src/styles/dialogs.css`), the edit-paper dialog `crates/easyoffice-app/ui/doc-page-setup.js` (the paper list, the field layout and the body-size check of `rhwp-studio/src/ui/page-setup-dialog.ts` and `rhwp-studio/src/core/page-body-limits.ts`, rewritten as plain JavaScript) with the section, margin grid and orientation card rules of `shell.css` (`.dialog-section`, `.margin-grid`, `.icon-radio-card` from `dialogs.css`). Each of these files starts with a header naming its source.

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

## iwork crate (MIT, reference implementation of the Pages table merge formula store)

Source: https://crates.io/crates/iwork (version 0.4.0, `src/table.rs` `merges_from_owner`, `FORMAT.md` sections 5 and 8).
Only the reading of table merge regions from the merge owner's formula store (colon tract and single cell reference nodes) and the notes on footnote, column, hyperlink and tracked-change layouts in `crates/easyoffice-formats/src/pages_schema.rs` were taken from it, re-implemented against this repository's protobuf reader with added budgets (`crates/easyoffice-formats/src/pages_table/merge.rs`, `rich_read_pages/marks.rs`). Every constant that no fixture of ours confirms is marked as unverified in `pages_schema.rs`.

```
MIT License

Copyright (c) 2026 Zebra Pig

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

## iwork-rs (MIT, reference implementation of Pages document reading and package writing)

Source: https://github.com/zebra-pig/iwork-rs (`FORMAT.md`, `src/create.rs`, `src/iwa.rs`, `src/references.rs`, `src/style.rs`, `src/document.rs`, `src/pages.rs`, `src/annotations.rs`, `src/chart.rs`, `src/drawable.rs`, `src/text.rs`, `src/table.rs`).
The structure of the Pages package (objects, field numbers, component index, `Data/` `DataInfo` entries, variation styles, `override_count`, the table of references that are not owned, `TST.TableInfoArchive`/`TableModelArchive`/`Tile`/`TableDataList`/`MergeRegionMap`/`RichTextPayloadArchive`, `TSD.ImageArchive`, `TSWP.ListStyleArchive`/`ColumnStyleArchive`/`HyperlinkFieldArchive`/`NumberAttachmentArchive`, `TP.SettingsArchive`/`SectionArchive`/`HeaderFooterList`) and the rules for writing it were taken from it and re-implemented in `crates/easyoffice-formats/src/pages_write/` (`PD-162`, `PD-246`) with this repository's own encoders (protobuf, literal-only Snappy blocks, binary property list) and fail-closed resource budgets. For the Pages reader (PD-240), object/field layouts and wire rules were adapted with fail-closed byte/entry budgets into `crates/easyoffice-formats/src/pages_schema.rs`, `crates/easyoffice-formats/src/pages_table/cell_rich.rs` (`src/text.rs`, `src/table.rs`: cell hyperlinks and inline equation/picture attachments), `crates/easyoffice-formats/src/rich_read_pages/sections.rs` (`src/pages.rs`: page setup, section header/footer 3-zone strips, first/even/odd templates, `NumberAttachmentArchive` page fields, and `ColumnStyleArchive` layouts), `crates/easyoffice-formats/src/rich_read_pages/changes.rs` (`src/annotations.rs`: `ChangeArchive`, `ChangeSessionArchive`, `AnnotationAuthorArchive`, and body insertion/deletion run tables), `crates/easyoffice-formats/src/rich_read_pages/equations.rs` (`src/drawable.rs`, `FORMAT.md`: `ImageArchive` equation extensions `100..=103` and `EquationInfoArchive`), and `crates/easyoffice-formats/src/rich_read_pages/charts.rs` (`src/chart.rs`: `ChartDrawableArchive` extension `10000`, 28 chart types, and `ChartGridArchive` blank `GridValue` preservation). Each adapted file starts with a header naming its source. No vendor `.proto` files, templates or generated data were taken; fonts, sizes, colours, margins and palette of the writer are this project's own values.

```
MIT License

Copyright (c) 2026 Zebra Pig

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

## formualizer (ported functions, MIT OR Apache-2.0, used under MIT)

Source: https://github.com/PSU3D0/formualizer (commit `214bf226e4a4ee4927d96f3e9cb42ef742fa61e7`). Nothing is vendored or linked; the incremental recalculation graph of `easyoffice-sheet` (`crates/easyoffice-sheet/src/recalc/`, PD-164) is a port of these functions, rewritten without the reference's Arrow or workbook dependencies and with different data layouts (each file starts with a header naming the source path and the changes):

- `crates/formualizer-eval/src/engine/idset.rs` (`DenseIdSet`) -> `recalc/id_set.rs`.
- `crates/formualizer-eval/src/engine/interval_tree.rs` (`IntervalTree`) -> `recalc/interval.rs`.
- `crates/formualizer-eval/src/engine/graph/range_deps.rs` (range registration and candidate lookup) and `graph/mod.rs` (`mark_dirty_many`, the dirty closure) -> `recalc/graph.rs`.

```
MIT License

Copyright (c) 2025-2026 Frank Colson and Formualizer contributors

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

## univer-office-copilot (Apache-2.0)

The per-turn change list of the copilot panel (`web/src/features/copilot/turn-changes.tsx`, commit `3dfa757d8643b6f3d2072daf5c4357d2c6a3f451`: the item list with a place label and a kind tag, the before and after sides of each item, the "show more" limit and the locate button, lines 84-141 and 309-318) was ported into `crates/easyoffice-app/ui/copilot-preview.js` as plain DOM and `copilot.css` (`PD-268`); the original compares two saved file versions, this one reads the places a model's operations will change.

Source: https://github.com/dream-num/univer-office-copilot (Copyright DreamNum Co., Ltd.).
Nothing is vendored or linked at runtime; the right-hand Copilot sidebar interaction patterns (`web/src/app/copilot-panel.tsx` bounded pointer/keyboard width resizing, `web/src/features/copilot/assistant-ui/welcome.tsx` and `thread.tsx` empty-state examples, in-place update status chips, bottom-row composer layout, `web/src/features/copilot/model-picker.tsx` in-composer model & reasoning-effort picker, `web/src/features/settings/index.tsx` model settings dialog, and `server/src/modules/providers/{index,models,endpoint}.ts` SSRF validation and `/models` discovery) were adapted as plain JavaScript, token-driven CSS, and safe Rust in `crates/easyoffice-app/ui/{copilot.css,copilot-catalog.js,copilot-actions.js,copilot-model-ui.js,copilot-ui.js}`, `crates/easyoffice-app/src/copilot/`, and `crates/easyoffice-desktop/src/copilot_http.rs` (`PD-253`). The request-id keyed cancel of a running model request (`server/src/modules/copilot/index.ts`, one abort handle per request) is the design behind `crates/easyoffice-app/src/copilot/cancel.rs` (`PD-267`); the Rust version closes the request's socket instead of aborting a fetch, and no source lines were copied. Each adapted file starts with a header naming its source.

```
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS
```

## continue (Apache-2.0)

For the second copilot round (`PD-268`, commit `5522c6f44ca0ac3528b37244818fbfa39b5af470`), `core/llm/countTokens.ts:282-336` (`pruneLinesFromTop`, `pruneLinesFromBottom`) and `core/autocomplete/templating/index.ts:251-285` (the excess shared between the text before and after the cursor in proportion to their sizes) were ported into `crates/easyoffice-app/src/copilot/window.rs` with bytes instead of tokens, so the selection is sent first and the paragraphs around it fill what is left of the context. The stream delta shapes (`packages/fetch/src/stream.ts:54-99` `parseDataLine`/`parseSseLine`, `core/llm/llms/OpenAI.ts:686`, `core/llm/llms/Anthropic.ts:346-360`) and the JSON-array stream of the Gemini endpoint (`core/llm/llms/Gemini.ts:369-455` `processGeminiResponse`) were ported into `crates/easyoffice-app/src/copilot/stream.rs`.

Source: https://github.com/continuedev/continue (Copyright Continue Dev, Inc.).
Nothing is vendored or linked at runtime; the company-by-company provider and model preset definitions (`packages/llm-info/src/providers/{openai,anthropic,gemini,mistral,xAI,ollama}.ts`) and the wire protocol adapters (`core/llm/llms/{Anthropic,Gemini,OpenAI,Deepseek,OpenRouter,Groq,Mistral,xAI,Ollama,LMStudio}.ts`: Anthropic `/v1/messages` with `x-api-key`, `anthropic-version`, alternating turn coalescing, and `thinking` token budgets; Google `/v1beta/models/{model}:generateContent` with `x-goog-api-key` header, `systemInstruction`, consecutive role merging, and `thinkingConfig`; OpenAI-compatible `/chat/completions` with `reasoning_effort` and `/models` discovery) were adapted into `crates/easyoffice-app/src/copilot/{providers,llm_client,types}.rs` and `crates/easyoffice-app/ui/copilot-catalog.js` (`PD-253`). Each adapted file starts with a header naming its source, and the full Apache License 2.0 text immediately above applies identically.

```
Copyright 2023-2026 Continue Dev, Inc.
Licensed under the Apache License, Version 2.0 (see full text above).
```

## novel (Apache-2.0)

Source: https://github.com/steven-tey/novel (commit `fa95098e66476c466faebb8211baa5869c101a9c`, Copyright Steven Tey). Nothing is vendored or linked at runtime. The selection command set (improve, fix, shorter, longer acting on the selected text) is taken from `apps/web/components/tailwind/generative/ai-selector-commands.tsx:5-26`, and the role sentence of each instruction from `apps/web/app/api/generate/route.ts:54-102` (`improve`, `shorter`, `longer`, `fix`), translated into Korean in `crates/easyoffice-app/src/copilot/prompt.rs` (`command_directive`, `PD-267`). The "no more than 200 characters" and "use Markdown" sentences were left out because the result replaces whole paragraphs. The license text is the Apache License 2.0 reproduced above.

## plate (MIT)

For the second copilot round (`PD-268`), the word-level text diff (`packages/platejs/src/ai/react/aiOutputLanding.internal.ts:212-387`: `diffSequences`, `textTokens`, `textEdits`) and the accept/reject decision of the pending change (`packages/platejs/src/ai/react/AIChatPlugin.ts:449-470`, commit `7f423b58dc7b19ce1170bee7c5824dda4f54d27d`) were ported into `crates/easyoffice-app/ui/copilot-diff.js`, and the Markdown joiner that holds a half-written mark until it closes (`apps/www/src/registry/lib/markdown-joiner-transform.ts:60-233`, stable branch commit `e401aaf96b4b23cb524096ec4f118046e8ba1c6b`) into `crates/easyoffice-app/ui/copilot-stream.js`. `streamInsertChunk` (`packages/ai/src/react/ai-chat/streaming/streamInsertChunk.ts`) was read and not ported: it deserializes streamed Markdown into editor nodes, and the copilot streams only the reply text and applies validated JSON operations.

Source: https://github.com/udecode/plate (commit `67de7de401b603c4a71e4283587286517348b093`, Copyright (c) Ziad Beyens, Dylan Schiemann, Joe Anderson, Felix Feng). Nothing is vendored or linked at runtime. The rule sentences of the selection commands (improve writing, fix spelling and grammar, make longer, make shorter, summarize, translate) are taken from `apps/www/src/registry/components/editor/ai-menu.tsx:516-584, 606-619`, `apps/www/src/registry/app/api/ai/command/prompt/common.ts:9-12`, `getGeneratePrompt.ts:75,142` and `getEditPrompt.ts:169`, translated into Korean in `crates/easyoffice-app/src/copilot/prompt.rs`. The regenerate rule (restore the target the request was made for, send nothing when it cannot be restored, then ask again) is ported from `reload` in `packages/platejs/src/ai/react/AIChatPlugin.ts:1703-1736` and `regenerate` in `packages/platejs/src/ai/react/useAIChat.ts:235-238` into `crates/easyoffice-app/ui/copilot-ui.js` (`regenerateLast`) and `copilot-actions.js` (`snapshotTarget`), with a text snapshot of the target in place of node keys (`PD-267`).

```
The MIT License (MIT)

Copyright (c) Ziad Beyens, Dylan Schiemann, Joe Anderson, Felix Feng

Unless otherwise specified in a LICENSE file within an individual package directory,
this license applies to all files in this repository outside of those package directories.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## unicode-substring (MIT)

Source: https://github.com/lautis/unicode-substring (commit `088e12b6197e1fd13f227a6b86b41044b7fe5f75`). The surrogate-pair test of `charAt` (`index.js:1-11`) and the code point walk of `slice` (`index.js:13-29`) are adapted into `clipCodePoints` in `crates/easyoffice-app/ui/copilot-text.js`, so a text sent to the copilot engine is cut on a code point (`PD-267`).

```
Copyright (c) 2015 Ville Lautanala

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## Excalidraw (MIT)

For the second copilot round (`PD-268`, commit `2c3dcc06ec807e81ab9437eceb1a8f7a3c590743`), the collaborator pointer (`packages/excalidraw/clients.ts:60-203` `renderRemoteCursors`: the arrow with the vertices (0,0), (0,14), (4,9), (11,8), the white outline of width 6 under the body of width 2, the label position and padding, the edge clamp with the 0.3 opacity and the hidden label, the press ring of radius 15), `easeOut` (`packages/common/src/utils.ts:253-255`) and the keyed frame loop (`packages/excalidraw/renderer/animation.ts:30-211` `AnimationController`) were ported into `crates/easyoffice-app/ui/copilot-cursor.js` and `copilot.css` as an SVG pointer and a vanilla frame loop. The original draws a remote pointer where the received coordinates say; the glide between two places uses the same loop with `easeOut`.

Source: https://github.com/excalidraw/excalidraw (commit `091bc0e9d86fef5727c3f4d148a73e441b064ed1`, Copyright (c) 2020 Excalidraw).
Nothing is vendored or linked at runtime; the non-uniform obstacle-padded grid A* orthogonal elbow arrow router (`packages/element/src/elbowArrow.ts`: `BinaryHeap` with `rescoreElement`, 16-case `estimateSegmentCount` heuristic, reverse-heading prohibition `flipHeading`, segment midpoint obstacle rejection, `removeElbowArrowShortSegments`, and `getElbowArrowCornerPoints`) and the floating island surface layout (`packages/excalidraw/components/Island.scss`) were adapted into `crates/easyoffice-app/ui/{board-router.js,board-studio.js,board.css}` (`PD-260`).

```
MIT License

Copyright (c) 2020 Excalidraw

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

## typed.js (MIT, release v2.1.0)

Source: https://github.com/mattboldt/typed.js (tag `v2.1.0`, commit `3daa3a4760dff3c563964ef0935b64ccfb0b22f8`, Copyright (c) 2023 Matt Boldt). Later releases are under the GPL-3.0 and are not used. Nothing is vendored or linked at runtime. The typing loop (`src/typed.js:30-57` stop and start, `:89-110` begin, `:112-196` typewrite, `:198-217` keepTyping, `:219-241` doneTyping, `:318-325` setPauseStatus, `:329-345` toggleBlinking, `:347-353` humanizer) was ported into `crates/easyoffice-app/ui/copilot-typing.js` (`PD-268`) as a callback loop that works on code points. The `^N` pause and the backtick skip syntax were left out because they would eat those characters from a document's text.

```
The MIT License (MIT)

Copyright (c) 2023 Matt Boldt

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

## y-prosemirror (MIT)

Source: https://github.com/yjs/y-prosemirror (commit `47a7385201bcefb55fcd5f1c8df23f48f46fa223`, Copyright (c) 2019 Kevin Jahns). Nothing is vendored or linked at runtime. The remote cursor of `src/cursor-plugin.js:25-62` (`defaultCursorBuilder`: the caret bar with a name label, `defaultSelectionBuilder`: a selection coloured with the user colour, the `maxsize` clamp of positions) and the demo styles `demo/prosemirror.html:36-68` (a caret with a 1px line on each side, the selection at 0.3 opacity) were re-expressed in `crates/easyoffice-app/ui/copilot-cursor.js` (`setSelection`, `setCaret`, `setIdle`) and `copilot.css` as overlay boxes drawn over the editor instead of decorations inside it (`PD-268`).

```
The MIT License (MIT)

Copyright (c) 2019 Kevin Jahns <kevin.jahns@protonmail.com>.

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

## LibreChat (MIT)

Source: https://github.com/danny-avila/LibreChat (commit `4700c44eb77c142b4f9f8b87476924f22890eb39`, Copyright (c) 2026 LibreChat). Nothing is vendored or linked at runtime. No lines were copied. The `final` and `failureSeen` rule of `packages/chat/src/core/transport/sse.ts:176-258` (`reconnectToStream`: a stream that ends before its final frame is an error, and an error already told is not told again) and the skipping of a frame of the wrong shape (`emitFrame`, `:22-35`) are followed by `createStreamSession` in `crates/easyoffice-app/ui/copilot-stream.js` (`PD-268`). The original re-attaches to a server-side stream; a copilot request cannot be re-attached, so the page keeps the text it has and takes the whole answer when it arrives.

```
MIT License

Copyright (c) 2026 LibreChat

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

## partial-json-parser-js (MIT)

Source: https://github.com/promplate/partial-json-parser-js (commit `86fd4bcbff1e32f80dd33e62318221702905a8f0`, Copyright (c) 2023 Promplate Dev Team). Nothing is vendored or linked at runtime. The incomplete-JSON reader (`src/index.ts:7-189`, `src/options.ts`) was ported into `parseJSON` in `crates/easyoffice-app/ui/copilot-stream.js` (`PD-268`) with every partial type allowed and the infinity and NaN forms left out, to show the reply of a model's JSON answer while it is still being written.

```
MIT License

Copyright (c) 2023 Promplate Dev Team

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

## Tauri JavaScript API (MIT OR Apache-2.0, used under MIT)

Source: https://github.com/tauri-apps/tauri (commit `9bc364d8bfec2abd377268fb408e94d3c711cc19`, Copyright (c) 2017 - Present Tauri Apps Contributors). Nothing is vendored or linked at runtime. `packages/api/src/core.ts:69-75, 119-222` (`transformCallback`, `Channel`: messages carry the index they were sent with and a later one waits for the missing ones) was ported into `createStreamChannel` in `crates/easyoffice-app/ui/copilot-stream.js` without the `@tauri-apps/api` bundle (`PD-268`). The Rust side uses `tauri::ipc::JavaScriptChannelId`, a dependency the shell already has.

```
MIT License

Copyright (c) 2017 - Present Tauri Apps Contributors

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

## xyflow (MIT)

Source: https://github.com/xyflow/xyflow (commit `29ab9e001c2ba320c9d84b00c729c745132d9145`, Copyright (c) 2019-2025 webkid GmbH).
Nothing is vendored or linked at runtime; the curvature-aware cubic Bezier edge control offset and midpoint calculation (`packages/system/src/utils/edges/bezier-edge.ts`: `calculateControlOffset`, `getControlWithCurvature`, `getBezierEdgeCenter`), smooth step corner rounding (`packages/system/src/utils/edges/smoothstep-edge.ts`: `getBend`), and node handle / resizer / bottom-left viewport controls chrome (`packages/system/src/styles/{style,node-resizer}.css`) were adapted into `crates/easyoffice-app/ui/{board-router.js,board-studio.js,board.css}` (`PD-260`).

```
MIT License

Copyright (c) 2019-2025 webkid GmbH

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

## TanStack Table (MIT)

Source: https://github.com/TanStack/table (commit `1cfb2e57cbfecf25e3d4ee1bda32fbd65e4e0327`, Copyright (c) 2016 Tanner Linsley).
Nothing is vendored or linked at runtime; the zero-allocation chunked alphanumeric comparator (`packages/table-core/src/features/row-sorting/sortFns.ts`: `compareAlphanumeric`, `findChunkEnd`, `compareNumericChunks`, `parseSmallInt`, `countRemainingChunks`), column filter predicates (`packages/table-core/src/features/column-filtering/filterFns.ts`), and grouped row model bucket folding (`packages/table-core/src/features/column-grouping/createGroupedRowModel.ts`) were adapted into `crates/easyoffice-app/ui/base-model.js` (`PD-259`).

```
MIT License

Copyright (c) 2016 Tanner Linsley

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

## glide-data-grid (MIT)

Source: https://github.com/glideapps/glide-data-grid (commit `0875d78cc41535a39f8b417ff88ce1ebe129c7451` / `0875d78cc41535a39f8b417ff88ce1ebe129c363`, Copyright (c) 2021 typeguard, Inc.).
Nothing is vendored or linked at runtime; the datasheet row marker cell, pill bubble badge cell, and trailing inline new-row cell surface patterns (`packages/core/src/cells/{bubble-cell,marker-cell,new-row-cell}.tsx`) were adapted into `crates/easyoffice-app/ui/{base-studio.js,base.css}` (`PD-259`).

```
MIT License

Copyright (c) 2021 typeguard, Inc.

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

These are the crates statically linked into `vendor/rhwp-core/rhwp_bg.wasm` (built from the pinned rhwp commit with Rust 1.93.1 and wasm-pack 0.14.0, see `vendor/rhwp-core/PROVENANCE.json`). The list and texts below are generated offline by `scripts/generate-wasm-notices.mjs` from `cargo metadata` and the crates' own license and NOTICE files, and must be regenerated whenever the WebAssembly package is rebuilt.

<!-- BEGIN GENERATED WASM DEPENDENCY NOTICES -->

149 third-party crates are linked into the module (normal dependencies of rhwp for wasm32-unknown-unknown;
build-time and test-only crates are not shipped).

| Crate | Version | License |
|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| aead | 0.6.1 | MIT OR Apache-2.0 |
| aes | 0.9.3 | MIT OR Apache-2.0 |
| argon2 | 0.6.0 | MIT OR Apache-2.0 |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| base64ct | 1.8.3 | Apache-2.0 OR MIT |
| bitflags | 2.13.1 | MIT OR Apache-2.0 |
| blake2 | 0.11.0 | MIT OR Apache-2.0 |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 |
| block-padding | 0.4.2 | MIT OR Apache-2.0 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| bytemuck_derive | 1.12.0 | Zlib OR Apache-2.0 OR MIT |
| byteorder | 1.5.0 | Unlicense OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| cbc | 0.2.1 | MIT OR Apache-2.0 |
| cfb | 0.15.0 | MIT |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 |
| chacha20 | 0.10.1 | MIT OR Apache-2.0 |
| chacha20poly1305 | 0.11.0 | Apache-2.0 OR MIT |
| cipher | 0.5.2 | MIT OR Apache-2.0 |
| cmov | 0.5.4 | Apache-2.0 OR MIT |
| codepage | 0.1.3 | Apache-2.0 OR MIT |
| color_quant | 1.1.0 | MIT |
| console_error_panic_hook | 0.1.7 | Apache-2.0/MIT |
| const-oid | 0.10.2 | Apache-2.0 OR MIT |
| constant_time_eq | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| core_maths | 0.1.1 | MIT |
| cpubits | 0.1.1 | MIT OR Apache-2.0 |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 |
| ctutils | 0.4.2 | Apache-2.0 OR MIT |
| curve25519-dalek | 5.0.0 | BSD-3-Clause |
| der | 0.8.1 | Apache-2.0 OR MIT |
| des | 0.9.0 | MIT OR Apache-2.0 |
| digest | 0.11.3 | MIT OR Apache-2.0 |
| ed25519 | 3.0.0 | Apache-2.0 OR MIT |
| ed25519-dalek | 3.0.0 | BSD-3-Clause |
| embedded-io | 0.7.1 | MIT OR Apache-2.0 |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |
| euclid | 0.22.14 | MIT OR Apache-2.0 |
| fax | 0.2.7 | MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| fnv | 1.0.7 | Apache-2.0 / MIT |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| gif | 0.14.2 | MIT OR Apache-2.0 |
| half | 2.7.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hmac | 0.13.0 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.14 | MIT OR Apache-2.0 |
| image | 0.25.10 | MIT OR Apache-2.0 |
| indexmap | 2.14.0 | Apache-2.0 OR MIT |
| inout | 0.2.2 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| js-sys | 0.3.104 | MIT OR Apache-2.0 |
| keccak | 0.2.1 | Apache-2.0 OR MIT |
| kem | 0.3.0 | Apache-2.0 OR MIT |
| kurbo | 0.13.1 | Apache-2.0 OR MIT |
| libm | 0.2.16 | MIT |
| log | 0.4.34 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| ml-dsa | 0.1.1 | Apache-2.0 OR MIT |
| ml-kem | 0.3.2 | Apache-2.0 OR MIT |
| module-lattice | 0.2.3 | Apache-2.0 OR MIT |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| password-hash | 0.6.1 | MIT OR Apache-2.0 |
| paste | 1.0.15 | MIT OR Apache-2.0 |
| pbkdf2 | 0.13.0 | MIT OR Apache-2.0 |
| pcx | 0.2.5 | MIT OR Apache-2.0 OR WTFPL |
| phc | 0.6.1 | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pkcs8 | 0.11.0 | Apache-2.0 OR MIT |
| png | 0.18.1 | MIT OR Apache-2.0 |
| poly1305 | 0.9.1 | Apache-2.0 OR MIT |
| polycool | 0.4.0 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quick-error | 2.0.1 | MIT/Apache-2.0 |
| quick-xml | 0.42.0 | MIT |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 |
| roxmltree | 0.21.1 | MIT OR Apache-2.0 |
| rustybuzz | 0.20.1 | MIT |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| sha1 | 0.11.0 | MIT OR Apache-2.0 |
| sha2 | 0.11.0 | MIT OR Apache-2.0 |
| sha3 | 0.11.0 | MIT OR Apache-2.0 |
| shake | 0.1.0 | MIT OR Apache-2.0 |
| signature | 3.0.0 | Apache-2.0 OR MIT |
| simd-adler32 | 0.3.10 | MIT |
| siphasher | 1.0.3 | MIT/Apache-2.0 |
| slab | 0.4.12 | MIT |
| smallvec | 1.15.2 | MIT OR Apache-2.0 |
| snafu | 0.9.2 | MIT OR Apache-2.0 |
| snafu-derive | 0.9.2 | MIT OR Apache-2.0 |
| spki | 0.8.0 | Apache-2.0 OR MIT |
| sponge-cursor | 0.1.0 | MIT OR Apache-2.0 |
| strum | 0.28.0 | MIT |
| strum_macros | 0.28.0 | MIT |
| subtle | 2.6.1 | BSD-3-Clause |
| svgtypes | 0.16.1 | Apache-2.0 OR MIT |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.3 | MIT OR Apache-2.0 |
| tiff | 0.11.3 | MIT |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 |
| typed-path | 0.12.3 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 |
| unicode-ccc | 0.4.0 | MIT/Apache-2.0 |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-properties | 0.1.4 | MIT/Apache-2.0 |
| unicode-script | 0.5.8 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 |
| universal-hash | 0.6.1 | MIT OR Apache-2.0 |
| uuid | 1.24.1 | Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.127 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.127 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.127 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.127 | MIT OR Apache-2.0 |
| web-sys | 0.3.104 | MIT OR Apache-2.0 |
| web-time | 1.1.0 | MIT OR Apache-2.0 |
| weezl | 0.1.12 | MIT OR Apache-2.0 |
| zerocopy | 0.8.56 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.56 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |
| zip | 8.6.0 | MIT |
| zlib-rs | 0.6.7 | Zlib |
| zmij | 1.0.23 | MIT |
| zopfli | 0.8.3 | Apache-2.0 |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib |

### License texts

#### Text 1

Applies to: adler2 2.0.1

```
Copyright (C) Jonas Schievink <jonasschievink@gmail.com>

Permission to use, copy, modify, and/or distribute this software for
any purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN
AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT
OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

#### Text 2

Applies to: adler2 2.0.1

```
Apache License
                        Version 2.0, January 2004
                     https://www.apache.org/licenses/LICENSE-2.0

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 3

Applies to: adler2 2.0.1, itoa 1.0.18, once_cell 1.21.4, paste 1.0.15, pin-project-lite 0.2.17, proc-macro2 1.0.107, quote 1.0.47, serde 1.0.229, serde_core 1.0.229, serde_derive 1.0.229, serde_json 1.0.151, syn 2.0.119, syn 3.0.3, typed-path 0.12.3, unicode-ident 1.0.24, zmij 1.0.23

```
Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 4

Applies to: aead 0.6.1, aes 0.9.3, argon2 0.6.0, base64ct 1.8.3, blake2 0.11.0, block-buffer 0.12.1, block-padding 0.4.2, cbc 0.2.1, chacha20 0.10.1, chacha20poly1305 0.11.0, cipher 0.5.2, const-oid 0.10.2, cpubits 0.1.1, crypto-common 0.2.2, der 0.8.1, digest 0.11.3, hmac 0.13.0, hybrid-array 0.4.14, inout 0.2.2, keccak 0.2.1, ml-kem 0.3.2, module-lattice 0.2.3, password-hash 0.6.1, pbkdf2 0.13.0, phc 0.6.1, pkcs8 0.11.0, poly1305 0.9.1, sha1 0.11.0, sha2 0.11.0, sha3 0.11.0, shake 0.1.0, signature 3.0.0, spki 0.8.0, sponge-cursor 0.1.0, universal-hash 0.6.1

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 5

Applies to: aead 0.6.1

```
Copyright (c) 2019-2026 The RustCrypto Project Developers
Copyright (c) 2019 MobileCoin, LLC

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 6

Applies to: aes 0.9.3

```
Copyright (c) 2018-2024 The RustCrypto Project Developers
Copyright (c) 2018 Artyom Pavlov

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 7

Applies to: argon2 0.6.0, phc 0.6.1, spki 0.8.0

```
Copyright (c) 2021-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 8

Applies to: arrayvec 0.7.8, base64 0.23.1, bitflags 2.13.1, bumpalo 3.20.3, cfg-if 1.0.4, embedded-io 0.7.1, equivalent 1.0.2, euclid 0.22.14, flate2 1.1.10, fnv 1.0.7, gif 0.14.2, hashbrown 0.17.1, heck 0.5.0, indexmap 2.14.0, js-sys 0.3.104, log 0.4.34, num-traits 0.2.19, once_cell 1.21.4, png 0.18.1, roxmltree 0.21.1, scopeguard 1.2.0, smallvec 1.15.2, svgtypes 0.16.1, ttf-parser 0.25.1, unicode-bidi-mirroring 0.4.0, unicode-ccc 0.4.0, unicode-properties 0.1.4, unicode-segmentation 1.13.3, unicode-width 0.2.2, uuid 1.24.1, wasm-bindgen 0.2.127, wasm-bindgen-macro 0.2.127, wasm-bindgen-macro-support 0.2.127, wasm-bindgen-shared 0.2.127, web-sys 0.3.104, weezl 0.1.12

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 9

Applies to: arrayvec 0.7.8

```
Copyright (c) Ulrik Sverdrup "bluss" 2015-2023

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 10

Applies to: base64 0.23.1

```
The MIT License (MIT)

Copyright (c) 2025 Alice Maz, Marshall Pierce

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

#### Text 11

Applies to: base64ct 1.8.3

```
Copyright (c) 2014 Steve "Sc00bz" Thomas (steve at tobtu dot com)
Copyright (c) 2021-2025 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 12

Applies to: bitflags 2.13.1, log 0.4.34, num-traits 0.2.19

```
Copyright (c) 2014 The Rust Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 13

Applies to: blake2 0.11.0

```
Copyright (c) 2017-2026 The RustCrypto Project Developers
Copyright (c) 2017 Artyom Pavlov
Copyright (c) 2015-2016 The blake2-rfc Developers, Cesar Barros

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 14

Applies to: blake3 1.8.7

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright 2019 Jack O'Connor and Samuel Neves

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

#### Text 15

Applies to: blake3 1.8.7

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

    TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

    1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

    2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

    3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

    4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

    5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

    6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

    7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

    8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

    9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

    END OF TERMS AND CONDITIONS

    APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

    Copyright 2019 Jack O'Connor and Samuel Neves

    Licensed under the Apache License, Version 2.0 (the "License");
    you may not use this file except in compliance with the License.
    You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

    Unless required by applicable law or agreed to in writing, software
    distributed under the License is distributed on an "AS IS" BASIS,
    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
    See the License for the specific language governing permissions and
    limitations under the License.


---- LLVM Exceptions to the Apache 2.0 License ----

As an exception, if, as a result of your compiling your source code, portions
of this Software are embedded into an Object form of such source code, you
may redistribute such embedded portions in such Object form without complying
with the conditions of Sections 4(a), 4(b) and 4(d) of the License.

In addition, if you combine or link compiled forms of this Software with
software that is licensed under the GPLv2 ("Combined Software") and if a
court of competent jurisdiction determines that the patent provision (Section
3), the indemnity provision (Section 9) or other Section of the License
conflicts with the conditions of the GPLv2, you may retroactively and
prospectively choose to deem waived or otherwise exclude such Section(s) of
the License, but only in their entirety and only with respect to the Combined
Software.
```

#### Text 16

Applies to: blake3 1.8.7, constant_time_eq 0.4.2

```
Creative Commons Legal Code

CC0 1.0 Universal

    CREATIVE COMMONS CORPORATION IS NOT A LAW FIRM AND DOES NOT PROVIDE
    LEGAL SERVICES. DISTRIBUTION OF THIS DOCUMENT DOES NOT CREATE AN
    ATTORNEY-CLIENT RELATIONSHIP. CREATIVE COMMONS PROVIDES THIS
    INFORMATION ON AN "AS-IS" BASIS. CREATIVE COMMONS MAKES NO WARRANTIES
    REGARDING THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS
    PROVIDED HEREUNDER, AND DISCLAIMS LIABILITY FOR DAMAGES RESULTING FROM
    THE USE OF THIS DOCUMENT OR THE INFORMATION OR WORKS PROVIDED
    HEREUNDER.

Statement of Purpose

The laws of most jurisdictions throughout the world automatically confer
exclusive Copyright and Related Rights (defined below) upon the creator
and subsequent owner(s) (each and all, an "owner") of an original work of
authorship and/or a database (each, a "Work").

Certain owners wish to permanently relinquish those rights to a Work for
the purpose of contributing to a commons of creative, cultural and
scientific works ("Commons") that the public can reliably and without fear
of later claims of infringement build upon, modify, incorporate in other
works, reuse and redistribute as freely as possible in any form whatsoever
and for any purposes, including without limitation commercial purposes.
These owners may contribute to the Commons to promote the ideal of a free
culture and the further production of creative, cultural and scientific
works, or to gain reputation or greater distribution for their Work in
part through the use and efforts of others.

For these and/or other purposes and motivations, and without any
expectation of additional consideration or compensation, the person
associating CC0 with a Work (the "Affirmer"), to the extent that he or she
is an owner of Copyright and Related Rights in the Work, voluntarily
elects to apply CC0 to the Work and publicly distribute the Work under its
terms, with knowledge of his or her Copyright and Related Rights in the
Work and the meaning and intended legal effect of CC0 on those rights.

1. Copyright and Related Rights. A Work made available under CC0 may be
protected by copyright and related or neighboring rights ("Copyright and
Related Rights"). Copyright and Related Rights include, but are not
limited to, the following:

  i. the right to reproduce, adapt, distribute, perform, display,
     communicate, and translate a Work;
 ii. moral rights retained by the original author(s) and/or performer(s);
iii. publicity and privacy rights pertaining to a person's image or
     likeness depicted in a Work;
 iv. rights protecting against unfair competition in regards to a Work,
     subject to the limitations in paragraph 4(a), below;
  v. rights protecting the extraction, dissemination, use and reuse of data
     in a Work;
 vi. database rights (such as those arising under Directive 96/9/EC of the
     European Parliament and of the Council of 11 March 1996 on the legal
     protection of databases, and under any national implementation
     thereof, including any amended or successor version of such
     directive); and
vii. other similar, equivalent or corresponding rights throughout the
     world based on applicable law or treaty, and any national
     implementations thereof.

2. Waiver. To the greatest extent permitted by, but not in contravention
of, applicable law, Affirmer hereby overtly, fully, permanently,
irrevocably and unconditionally waives, abandons, and surrenders all of
Affirmer's Copyright and Related Rights and associated claims and causes
of action, whether now known or unknown (including existing as well as
future claims and causes of action), in the Work (i) in all territories
worldwide, (ii) for the maximum duration provided by applicable law or
treaty (including future time extensions), (iii) in any current or future
medium and for any number of copies, and (iv) for any purpose whatsoever,
including without limitation commercial, advertising or promotional
purposes (the "Waiver"). Affirmer makes the Waiver for the benefit of each
member of the public at large and to the detriment of Affirmer's heirs and
successors, fully intending that such Waiver shall not be subject to
revocation, rescission, cancellation, termination, or any other legal or
equitable action to disrupt the quiet enjoyment of the Work by the public
as contemplated by Affirmer's express Statement of Purpose.

3. Public License Fallback. Should any part of the Waiver for any reason
be judged legally invalid or ineffective under applicable law, then the
Waiver shall be preserved to the maximum extent permitted taking into
account Affirmer's express Statement of Purpose. In addition, to the
extent the Waiver is so judged Affirmer hereby grants to each affected
person a royalty-free, non transferable, non sublicensable, non exclusive,
irrevocable and unconditional license to exercise Affirmer's Copyright and
Related Rights in the Work (i) in all territories worldwide, (ii) for the
maximum duration provided by applicable law or treaty (including future
time extensions), (iii) in any current or future medium and for any number
of copies, and (iv) for any purpose whatsoever, including without
limitation commercial, advertising or promotional purposes (the
"License"). The License shall be deemed effective as of the date CC0 was
applied by Affirmer to the Work. Should any part of the License for any
reason be judged legally invalid or ineffective under applicable law, such
partial invalidity or ineffectiveness shall not invalidate the remainder
of the License, and in such case Affirmer hereby affirms that he or she
will not (i) exercise any of his or her remaining Copyright and Related
Rights in the Work or (ii) assert any associated claims and causes of
action with respect to the Work, in either case contrary to Affirmer's
express Statement of Purpose.

4. Limitations and Disclaimers.

 a. No trademark or patent rights held by Affirmer are waived, abandoned,
    surrendered, licensed or otherwise affected by this document.
 b. Affirmer offers the Work as-is and makes no representations or
    warranties of any kind concerning the Work, express, implied,
    statutory or otherwise, including without limitation warranties of
    title, merchantability, fitness for a particular purpose, non
    infringement, or the absence of latent or other defects, accuracy, or
    the present or absence of errors, whether or not discoverable, all to
    the greatest extent permissible under applicable law.
 c. Affirmer disclaims responsibility for clearing rights of other persons
    that may apply to the Work or any use thereof, including without
    limitation any person's Copyright and Related Rights in the Work.
    Further, Affirmer disclaims responsibility for obtaining any necessary
    consents, permissions or other rights required for any use of the
    Work.
 d. Affirmer understands and acknowledges that Creative Commons is not a
    party to this document and has no duty or obligation with respect to
    this CC0 or use of the Work.
```

#### Text 17

Applies to: block-buffer 0.12.1, block-padding 0.4.2

```
Copyright (c) 2018-2025 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 18

Applies to: bumpalo 3.20.3

```
Copyright (c) 2019 Nick Fitzgerald

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 19

Applies to: bytemuck 1.25.2, bytemuck_derive 1.12.0

```
Apache License
Version 2.0, January 2004
http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

    1. Definitions.

        "License" shall mean the terms and conditions for use, reproduction, and distribution as defined by Sections 1 through 9 of this document.

        "Licensor" shall mean the copyright owner or entity authorized by the copyright owner that is granting the License.

        "Legal Entity" shall mean the union of the acting entity and all other entities that control, are controlled by, or are under common control with that entity. For the purposes of this definition, "control" means (i) the power, direct or indirect, to cause the direction or management of such entity, whether by contract or otherwise, or (ii) ownership of fifty percent (50%) or more of the outstanding shares, or (iii) beneficial ownership of such entity.

        "You" (or "Your") shall mean an individual or Legal Entity exercising permissions granted by this License.

        "Source" form shall mean the preferred form for making modifications, including but not limited to software source code, documentation source, and configuration files.

        "Object" form shall mean any form resulting from mechanical transformation or translation of a Source form, including but not limited to compiled object code, generated documentation, and conversions to other media types.

        "Work" shall mean the work of authorship, whether in Source or Object form, made available under the License, as indicated by a copyright notice that is included in or attached to the work (an example is provided in the Appendix below).

        "Derivative Works" shall mean any work, whether in Source or Object form, that is based on (or derived from) the Work and for which the editorial revisions, annotations, elaborations, or other modifications represent, as a whole, an original work of authorship. For the purposes of this License, Derivative Works shall not include works that remain separable from, or merely link (or bind by name) to the interfaces of, the Work and Derivative Works thereof.

        "Contribution" shall mean any work of authorship, including the original version of the Work and any modifications or additions to that Work or Derivative Works thereof, that is intentionally submitted to Licensor for inclusion in the Work by the copyright owner or by an individual or Legal Entity authorized to submit on behalf of the copyright owner. For the purposes of this definition, "submitted" means any form of electronic, verbal, or written communication sent to the Licensor or its representatives, including but not limited to communication on electronic mailing lists, source code control systems, and issue tracking systems that are managed by, or on behalf of, the Licensor for the purpose of discussing and improving the Work, but excluding communication that is conspicuously marked or otherwise designated in writing by the copyright owner as "Not a Contribution."

        "Contributor" shall mean Licensor and any individual or Legal Entity on behalf of whom a Contribution has been received by Licensor and subsequently incorporated within the Work.
    2. Grant of Copyright License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable copyright license to reproduce, prepare Derivative Works of, publicly display, publicly perform, sublicense, and distribute the Work and such Derivative Works in Source or Object form.
    3. Grant of Patent License. Subject to the terms and conditions of this License, each Contributor hereby grants to You a perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable (except as stated in this section) patent license to make, have made, use, offer to sell, sell, import, and otherwise transfer the Work, where such license applies only to those patent claims licensable by such Contributor that are necessarily infringed by their Contribution(s) alone or by combination of their Contribution(s) with the Work to which such Contribution(s) was submitted. If You institute patent litigation against any entity (including a cross-claim or counterclaim in a lawsuit) alleging that the Work or a Contribution incorporated within the Work constitutes direct or contributory patent infringement, then any patent licenses granted to You under this License for that Work shall terminate as of the date such litigation is filed.
    4. Redistribution. You may reproduce and distribute copies of the Work or Derivative Works thereof in any medium, with or without modifications, and in Source or Object form, provided that You meet the following conditions:
        (a) You must give any other recipients of the Work or Derivative Works a copy of this License; and
        (b) You must cause any modified files to carry prominent notices stating that You changed the files; and
        (c) You must retain, in the Source form of any Derivative Works that You distribute, all copyright, patent, trademark, and attribution notices from the Source form of the Work, excluding those notices that do not pertain to any part of the Derivative Works; and
        (d) If the Work includes a "NOTICE" text file as part of its distribution, then any Derivative Works that You distribute must include a readable copy of the attribution notices contained within such NOTICE file, excluding those notices that do not pertain to any part of the Derivative Works, in at least one of the following places: within a NOTICE text file distributed as part of the Derivative Works; within the Source form or documentation, if provided along with the Derivative Works; or, within a display generated by the Derivative Works, if and wherever such third-party notices normally appear. The contents of the NOTICE file are for informational purposes only and do not modify the License. You may add Your own attribution notices within Derivative Works that You distribute, alongside or as an addendum to the NOTICE text from the Work, provided that such additional attribution notices cannot be construed as modifying the License.

        You may add Your own copyright statement to Your modifications and may provide additional or different license terms and conditions for use, reproduction, or distribution of Your modifications, or for any such Derivative Works as a whole, provided Your use, reproduction, and distribution of the Work otherwise complies with the conditions stated in this License.
    5. Submission of Contributions. Unless You explicitly state otherwise, any Contribution intentionally submitted for inclusion in the Work by You to the Licensor shall be under the terms and conditions of this License, without any additional terms or conditions. Notwithstanding the above, nothing herein shall supersede or modify the terms of any separate license agreement you may have executed with Licensor regarding such Contributions.
    6. Trademarks. This License does not grant permission to use the trade names, trademarks, service marks, or product names of the Licensor, except as required for reasonable and customary use in describing the origin of the Work and reproducing the content of the NOTICE file.
    7. Disclaimer of Warranty. Unless required by applicable law or agreed to in writing, Licensor provides the Work (and each Contributor provides its Contributions) on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied, including, without limitation, any warranties or conditions of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A PARTICULAR PURPOSE. You are solely responsible for determining the appropriateness of using or redistributing the Work and assume any risks associated with Your exercise of permissions under this License.
    8. Limitation of Liability. In no event and under no legal theory, whether in tort (including negligence), contract, or otherwise, unless required by applicable law (such as deliberate and grossly negligent acts) or agreed to in writing, shall any Contributor be liable to You for damages, including any direct, indirect, special, incidental, or consequential damages of any character arising as a result of this License or out of the use or inability to use the Work (including but not limited to damages for loss of goodwill, work stoppage, computer failure or malfunction, or any and all other commercial damages or losses), even if such Contributor has been advised of the possibility of such damages.
    9. Accepting Warranty or Additional Liability. While redistributing the Work or Derivative Works thereof, You may choose to offer, and charge a fee for, acceptance of support, warranty, indemnity, or other liability obligations and/or rights consistent with this License. However, in accepting such obligations, You may act only on Your own behalf and on Your sole responsibility, not on behalf of any other Contributor, and only if You agree to indemnify, defend, and hold each Contributor harmless for any liability incurred by, or claims asserted against, such Contributor by reason of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

To apply the Apache License to your work, attach the following boilerplate notice, with the fields enclosed by brackets "[]" replaced with your own identifying information. (Don't include the brackets!) The text should be enclosed in the appropriate comment syntax for the file format. We also recommend that a file or class name and description of purpose be included on the same "printed page" as the copyright notice for easier identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 20

Applies to: bytemuck 1.25.2, bytemuck_derive 1.12.0

```
MIT License

Copyright (c) 2019 Daniel "Lokathor" Gee.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice (including the next paragraph) shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

#### Text 21

Applies to: bytemuck 1.25.2, bytemuck_derive 1.12.0

```
Copyright (c) 2019 Daniel "Lokathor" Gee.

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### Text 22

Applies to: byteorder 1.5.0, memchr 2.8.3

```
This project is dual-licensed under the Unlicense and MIT licenses.

You may use this code under the terms of either license.
```

#### Text 23

Applies to: byteorder 1.5.0, byteorder-lite 0.1.0, memchr 2.8.3

```
The MIT License (MIT)

Copyright (c) 2015 Andrew Gallant

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

#### Text 24

Applies to: byteorder 1.5.0, byteorder-lite 0.1.0, memchr 2.8.3

```
This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <http://unlicense.org/>
```

#### Text 25

Applies to: cbc 0.2.1

```
Copyright (c) 2018-2022 RustCrypto Developers
Copyright (c) 2018 Artyom Pavlov

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 26

Applies to: cfb 0.15.0

```
MIT License

Copyright (c) 2017 Matthew D. Steele

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

#### Text 27

Applies to: cfg-if 1.0.4, js-sys 0.3.104, wasm-bindgen 0.2.127, wasm-bindgen-macro 0.2.127, wasm-bindgen-macro-support 0.2.127, wasm-bindgen-shared 0.2.127, web-sys 0.3.104

```
Copyright (c) 2014 Alex Crichton

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 28

Applies to: chacha20 0.10.1, chacha20poly1305 0.11.0

```
Copyright (c) 2019-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 29

Applies to: cipher 0.5.2

```
Copyright (c) 2016-2025 RustCrypto Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 30

Applies to: cmov 0.5.4, codepage 0.1.3, ctutils 0.4.2, encoding_rs 0.8.42, kurbo 0.13.1, multiversion_no_op 1.0.0, polycool 0.4.0, zeroize 1.9.0, zune-core 0.5.3, zune-jpeg 0.5.15

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

#### Text 31

Applies to: cmov 0.5.4, hybrid-array 0.4.14

```
Copyright (c) 2022-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 32

Applies to: codepage 0.1.3, encoding_rs 0.8.42, multiversion_no_op 1.0.0

```
Copyright Mozilla Foundation

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 33

Applies to: color_quant 1.1.0

```
The MIT License (MIT)

Copyright (c) 2016 PistonDevelopers

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

#### Text 34

Applies to: console_error_panic_hook 0.1.7

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

  http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 35

Applies to: console_error_panic_hook 0.1.7

```
Copyright (c) 2018 Nick Fitzgerald

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 36

Applies to: const-oid 0.10.2, der 0.8.1, pkcs8 0.11.0

```
Copyright (c) 2020-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 37

Applies to: constant_time_eq 0.4.2, itoa 1.0.18, paste 1.0.15, proc-macro2 1.0.107, quote 1.0.47, serde 1.0.229, serde_core 1.0.229, serde_derive 1.0.229, serde_json 1.0.151, syn 2.0.119, syn 3.0.3, typed-path 0.12.3, unicode-ident 1.0.24

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS
```

#### Text 38

Applies to: constant_time_eq 0.4.2

```
Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

#### Text 39

Applies to: core_maths 0.1.1

```
MIT License

Copyright (c) 2024 Robert Bastian

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

#### Text 40

Applies to: cpubits 0.1.1

```
Copyright (c) 2023-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 41

Applies to: crc32fast 1.5.2, quick-error 2.0.1

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "{}"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright {yyyy} {name of copyright owner}

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

#### Text 42

Applies to: crc32fast 1.5.2

```
MIT License

Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors

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

#### Text 43

Applies to: crypto-common 0.2.2, kem 0.3.0

```
Copyright (c) 2021-2026 RustCrypto Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 44

Applies to: ctutils 0.4.2

```
Copyright (c) 2025-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 45

Applies to: curve25519-dalek 5.0.0

```
Copyright (c) 2016-2021 isis agora lovecruft. All rights reserved.
Copyright (c) 2016-2021 Henry de Valence. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### Text 46

Applies to: des 0.9.0

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright 2017 Gulshan Singh

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 47

Applies to: des 0.9.0

```
Copyright (c) 2017-2024 The RustCrypto Project Developers
Copyright (c) 2017 Gulshan Singh, Antoni Boucher, Artyom Pavlov

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 48

Applies to: digest 0.11.3

```
Copyright (c) 2017-2025 RustCrypto Developers
Copyright (c) 2017 Artyom Pavlov

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 49

Applies to: ed25519 3.0.0

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright 2018-2022 RustCrypto Developers

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 50

Applies to: ed25519 3.0.0, signature 3.0.0

```
Copyright (c) 2018-2026 RustCrypto Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 51

Applies to: ed25519-dalek 3.0.0

```
Copyright (c) 2017-2019 isis agora lovecruft. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### Text 52

Applies to: embedded-io 0.7.1

```
Copyright (c) 2023 The embedded-io authors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 53

Applies to: encoding_rs 0.8.42

```
Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### Text 54

Applies to: equivalent 1.0.2

```
Copyright (c) 2016--2023

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 55

Applies to: euclid 0.22.14

```
Copyright (c) 2012-2013 Mozilla Foundation

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 56

Applies to: fax 0.2.7

```
Copyright © 2021 The pdf-rs contributers.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the “Software”), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

#### Text 57

Applies to: fdeflate 0.3.7, half 2.7.1, image 0.25.10, miniz_oxide 0.8.9, miniz_oxide 0.9.1, pin-project-lite 0.2.17

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS
```

#### Text 58

Applies to: fdeflate 0.3.7, image 0.25.10

```
MIT License

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 59

Applies to: flate2 1.1.10

```
Copyright (c) 2014-2026 Alex Crichton

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 60

Applies to: fnv 1.0.7

```
Copyright (c) 2017 Contributors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 61

Applies to: futures-core 0.3.34, futures-task 0.3.34, futures-util 0.3.34

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright (c) 2016 Alex Crichton
Copyright (c) 2017 The Tokio Authors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 62

Applies to: futures-core 0.3.34, futures-task 0.3.34, futures-util 0.3.34

```
Copyright (c) 2016 Alex Crichton
Copyright (c) 2017 The Tokio Authors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 63

Applies to: getrandom 0.4.3

```
Apache License
                        Version 2.0, January 2004
                     https://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 64

Applies to: getrandom 0.4.3

```
Copyright (c) 2018-2026 The rust-random Project Developers
Copyright (c) 2014 The Rust Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 65

Applies to: gif 0.14.2

```
The MIT License (MIT)

Copyright (c) 2015 nwin

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

#### Text 66

Applies to: half 2.7.1

```
MIT License

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

#### Text 67

Applies to: hashbrown 0.17.1

```
Copyright (c) 2016 Amanieu d'Antras

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 68

Applies to: heck 0.5.0, unicode-properties 0.1.4, unicode-segmentation 1.13.3, unicode-width 0.2.2

```
Copyright (c) 2015 The Rust Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 69

Applies to: hmac 0.13.0

```
Copyright (c) 2017 Artyom Pavlov

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 70

Applies to: indexmap 2.14.0

```
Copyright (c) 2016--2017

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 71

Applies to: inout 0.2.2

```
Copyright (c) 2022-2025 The RustCrypto Project Developers
Copyright (c) 2022 Artyom Pavlov

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 72

Applies to: keccak 0.2.1, zeroize 1.9.0

```
Copyright (c) 2018-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 73

Applies to: kem 0.3.0

```
Copyright 2021 RustCrypto Developers

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 74

Applies to: kurbo 0.13.1, polycool 0.4.0

```
Copyright (c) 2018 Raph Levien

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 75

Applies to: libm 0.2.16

```
rust-lang/libm as a whole is available for use under the MIT license:

------------------------------------------------------------------------------
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
------------------------------------------------------------------------------

As a contributor, you agree that your code can be used under either the MIT
license or the Apache-2.0 license:

------------------------------------------------------------------------------
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
------------------------------------------------------------------------------

This Rust library contains the following copyrights:

    Copyright (c) 2018 Jorge Aparicio

Portions of this software are derived from third-party works licensed under
terms compatible with the above MIT license:

* musl libc https://www.musl-libc.org/. This library contains the following
  copyright:

      Copyright © 2005-2020 Rich Felker, et al.

* The CORE-MATH project https://core-math.gitlabpages.inria.fr/. CORE-MATH
  routines are available under the MIT license on a per-file basis.

The musl libc COPYRIGHT file also includes the following notice relevant to
math portions of the library:

------------------------------------------------------------------------------
Much of the math library code (src/math/* and src/complex/*) is
Copyright © 1993,2004 Sun Microsystems or
Copyright © 2003-2011 David Schultz or
Copyright © 2003-2009 Steven G. Kargl or
Copyright © 2003-2009 Bruce D. Evans or
Copyright © 2008 Stephen L. Moshier or
Copyright © 2017-2018 Arm Limited
and labelled as such in comments in the individual source files. All
have been licensed under extremely permissive terms.
------------------------------------------------------------------------------

Copyright notices are retained in src/* files where relevant.
```

#### Text 76

Applies to: miniz_oxide 0.8.9, miniz_oxide 0.9.1

```
MIT License

Copyright 2013-2014 RAD Game Tools and Valve Software
Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC
Copyright (c) 2017 Frommi
Copyright (c) 2017-2024 oyvindln


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

#### Text 77

Applies to: miniz_oxide 0.8.9, miniz_oxide 0.9.1

```
MIT License

Copyright 2013-2014 RAD Game Tools and Valve Software
Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC
Copyright (c) 2017 Frommi
Copyright (c) 2017-2024 oyvindln

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

#### Text 78

Applies to: miniz_oxide 0.8.9, miniz_oxide 0.9.1

```
Copyright 2013-2014 RAD Game Tools and Valve Software
Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC
Copyright (c) 2020 Frommi
Copyright (c) 2017-2024 oyvindln

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### Text 79

Applies to: ml-dsa 0.1.1

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright 2024 Trail of Bits

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 80

Applies to: ml-dsa 0.1.1

```
Copyright (c) 2024-2026 RustCrypto Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 81

Applies to: ml-kem 0.3.2, module-lattice 0.2.3

```
Copyright (c) 2024-2026 RustCrypto Developers

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

#### Text 82

Applies to: moxcms 0.8.1, pxfm 0.1.30

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright 2024 Radzivon Bartoshyk

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

#### Text 83

Applies to: moxcms 0.8.1, pxfm 0.1.30

```
Copyright (c) Radzivon Bartoshyk. All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

1.  Redistributions of source code must retain the above copyright notice, this
    list of conditions and the following disclaimer.

2.  Redistributions in binary form must reproduce the above copyright notice,
    this list of conditions and the following disclaimer in the documentation
    and/or other materials provided with the distribution.

3.  Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived from
    this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### Text 84

Applies to: password-hash 0.6.1

```
Copyright (c) 2020-2026 RustCrypto Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 85

Applies to: pbkdf2 0.13.0

```
Copyright (c) 2017 Artyom Pavlov
Copyright (c) 2018-2023 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 86

Applies to: pcx 0.2.5

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright 2024 The PCX Crate Developers

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 87

Applies to: pcx 0.2.5

```
Copyright (c) 2024 The PCX Crate Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 88

Applies to: pcx 0.2.5

```
DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
                    Version 2, December 2004

 Everyone is permitted to copy and distribute verbatim or modified
 copies of this license document, and changing it is allowed as long
 as the name is changed.

            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
   TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION

0. You just DO WHAT THE FUCK YOU WANT TO.
```

#### Text 89

Applies to: png 0.18.1

```
Copyright (c) 2015 nwin

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 90

Applies to: poly1305 0.9.1

```
Copyright (c) 2015-2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 91

Applies to: quick-error 2.0.1

```
Copyright (c) 2015 The quick-error Developers

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

#### Text 92

Applies to: quick-xml 0.42.0

```
The MIT License (MIT)

Copyright (c) 2016 Johann Tuffe

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:


The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.


THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

#### Text 93

Applies to: rand_core 0.10.1

```
Apache License
                        Version 2.0, January 2004
                     https://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.
```

#### Text 94

Applies to: rand_core 0.10.1

```
Copyright (c) 2018-2026 The Rand Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 95

Applies to: roxmltree 0.21.1

```
The MIT License (MIT)

Copyright (c) 2018 Yevhenii Reizner

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

#### Text 96

Applies to: rustybuzz 0.20.1

```
The MIT License (MIT)

Copyright (c) HarfBuzz developers
Copyright (c) 2020 Yevhenii Reizner

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

#### Text 97

Applies to: scopeguard 1.2.0

```
Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 98

Applies to: sha1 0.11.0, sha2 0.11.0

```
Copyright (c) 2016-2026 The RustCrypto Project Developers
Copyright (c) 2016 Artyom Pavlov
Copyright (c) 2009-2013 Mozilla Foundation
Copyright (c) 2006-2009 Graydon Hoare

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 99

Applies to: sha3 0.11.0

```
Copyright (c) 2020-2026 The RustCrypto Project Developers
Copyright (c) 2016-2023 Artyom Pavlov, Marek Kotewicz
Copyright (c) 2014 Sébastien Martini
Copyright (c) 2009-2013 Mozilla Foundation
Copyright (c) 2006-2009 Graydon Hoare

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 100

Applies to: shake 0.1.0, sponge-cursor 0.1.0

```
Copyright (c) 2026 The RustCrypto Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 101

Applies to: simd-adler32 0.3.10

```
MIT License

Copyright (c) [2021] [Marvin Countryman]

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

#### Text 102

Applies to: siphasher 1.0.3

```
Copyright 2012-2016 The Rust Project Developers.
Copyright 2016-2026 Frank Denis.

Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
option.
```

#### Text 103

Applies to: slab 0.4.12

```
Copyright (c) 2019 Carl Lerche

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 104

Applies to: smallvec 1.15.2

```
Copyright (c) 2018 The Servo Project Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 105

Applies to: snafu 0.9.2, snafu-derive 0.9.2

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright 2019- Jake Goulding

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 106

Applies to: snafu 0.9.2, snafu-derive 0.9.2

```
Copyright (c) 2019- Jake Goulding

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 107

Applies to: strum 0.28.0, strum_macros 0.28.0

```
MIT License

Copyright (c) 2019 Peter Glotfelty

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

#### Text 108

Applies to: subtle 2.6.1

```
Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved.
Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### Text 109

Applies to: svgtypes 0.16.1, ttf-parser 0.25.1

```
Copyright (c) 2018 Yevhenii Reizner

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

#### Text 110

Applies to: tiff 0.11.3

```
MIT License

Copyright (c) 2018 PistonDevelopers

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

#### Text 111

Applies to: typenum 1.20.1

```
MIT OR Apache-2.0
```

#### Text 112

Applies to: typenum 1.20.1

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright 2014 Paho Lurie-Gregg

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 113

Applies to: typenum 1.20.1

```
The MIT License (MIT)

Copyright (c) 2014 Paho Lurie-Gregg

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

#### Text 114

Applies to: unicode-bidi-mirroring 0.4.0, unicode-ccc 0.4.0

```
The MIT License (MIT)

Copyright (c) 2020 Yevhenii Reizner

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

#### Text 115

Applies to: unicode-ident 1.0.24

```
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```

#### Text 116

Applies to: unicode-script 0.5.8

```
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright 2021 The Unicode-rs Developers

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### Text 117

Applies to: unicode-script 0.5.8

```
MIT License

Copyright (c) 2019 Manish Goregaokar

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 118

Applies to: universal-hash 0.6.1

```
Copyright (c) 2019-2025 RustCrypto Developers

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 119

Applies to: uuid 1.24.1

```
Copyright (c) 2014 The Rust Project Developers
Copyright (c) 2018 Ashley Mannix, Christopher Armstrong, Dylan DPC, Hunar Roop Kahlon

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 120

Applies to: web-time 1.1.0

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright 2023 dAxpeDDa

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

#### Text 121

Applies to: web-time 1.1.0

```
MIT License

Copyright (c) 2023 dAxpeDDa

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

#### Text 122

Applies to: weezl 0.1.12

```
The MIT License (MIT)

Copyright (c) HeroicKatora 2020

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

#### Text 123

Applies to: zerocopy 0.8.56, zerocopy-derive 0.8.56

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright 2023 The Fuchsia Authors

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

#### Text 124

Applies to: zerocopy 0.8.56, zerocopy-derive 0.8.56

```
Copyright 2019 The Fuchsia Authors.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### Text 125

Applies to: zerocopy 0.8.56, zerocopy-derive 0.8.56

```
Copyright 2023 The Fuchsia Authors

Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Text 126

Applies to: zip 8.6.0

```
The MIT License (MIT)

Copyright (c) 2014 Mathijs van de Nes

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

#### Text 127

Applies to: zlib-rs 0.6.7

```
(C) 2024 Trifecta Tech Foundation 

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### Text 128

Applies to: zopfli 0.8.3

```
Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright 2011 Google Inc.

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

#### Text 129

Applies to: zune-core 0.5.3, zune-jpeg 0.5.15

```
MIT License

Copyright (c) zune-image developers

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

#### Text 130

Applies to: zune-core 0.5.3, zune-jpeg 0.5.15

```
zlib License

(C) zune-image developers

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.
2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.
3. This notice may not be removed or altered from any source distribution.
```

<!-- END GENERATED WASM DEPENDENCY NOTICES -->

## JavaScript packages bundled into the editor

The editor bundle contains one third-party npm package besides the rhwp-studio sources: `@noble/hashes` 2.4.0 (MIT), pinned by `apps/hwp-studio/package-lock.json`.

```
The MIT License (MIT)

Copyright (c) 2022 Paul Miller (https://paulmillr.com)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the “Software”), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## Fonts

The editor bundle ships only fonts under the SIL Open Font License 1.1, pinned by size and SHA-256 in `assets/fonts/fonts.json` (see `assets/fonts/FONTS.md`). Families whose license reserves a font name (Nanum, D2Coding, Spoqa Han Sans, Pretendard) are the official distribution files, unchanged or with WOFF2 compression only, and their official source URL and SHA-256 are recorded in `fonts.json`; the other families are copied byte for byte from the rhwp checkout. The license text of every family is kept in `assets/fonts/licenses/` and is installed next to the fonts (`hwp-studio/fonts/licenses/`). The fonts are not sold on their own. Proprietary, operating-system and custom-terms fonts are never bundled. The desktop installer also ships `NotoSansKR-Regular.ttf` (Noto Sans KR Regular, the rhwp copy at `ttfs/opensource/`, a Hangul, Latin and symbol subset) with `NotoSansKR-OFL.txt` in `pdf-fonts/`, so that PDF export has a Korean face on a machine that has none (PD-114); it is the same family and license as the Noto Sans KR row below and is not sold on its own.

| Family | Styles | Copyright notice | License text |
|---|---|---|---|
| D2Coding | Bold, Regular | Copyright (c) 2015, NAVER Corporation (https://www.navercorp.com), | `assets/fonts/licenses/D2Coding-OFL.txt` |
| Gowun Batang | Bold, Regular | Copyright 2021 The Gowun Batang Project Authors (https://github.com/yangheeryu/Gowun-Batang) | `assets/fonts/licenses/GowunBatang-OFL.txt` |
| Gowun Dodum | Regular | Copyright 2021 The Gowun Dodum Project Authors (https://github.com/yangheeryu/Gowun-Dodum) | `assets/fonts/licenses/GowunDodum-OFL.txt` |
| Nanum Gothic | Bold, ExtraBold, Regular | Copyright (c) 2010, NHN Corporation (http://www.nhncorp.com), | `assets/fonts/licenses/NanumGothic-OFL.txt` |
| Nanum Gothic Coding | Bold, Regular | Copyright (c) 2010, NHN Corporation (http://www.nhncorp.com), | `assets/fonts/licenses/NanumGothicCoding-OFL.txt` |
| Nanum Myeongjo | Bold, ExtraBold, Regular | Copyright (c) 2010, NHN Corporation (http://www.nhncorp.com), | `assets/fonts/licenses/NanumMyeongjo-OFL.txt` |
| Noto Sans KR | Bold, ExtraLight, Regular | Copyright 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source' | `assets/fonts/licenses/NotoSansKR-OFL.txt` |
| Noto Serif KR | Bold, Regular | Copyright 2012 Google Inc. All Rights Reserved. | `assets/fonts/licenses/NotoSerifKR-OFL.txt` |
| Pretendard | Black, Bold, ExtraBold, ExtraLight, Light, Medium, Regular, SemiBold, Thin | Copyright (c) 2021, Kil Hyung-jin (https://github.com/orioncactus/pretendard), | `assets/fonts/licenses/Pretendard-OFL.txt` |
| Spoqa Han Sans | Regular | Copyright (c) 2015-10-09 Spoqa (spoqa.com), with Reserved Font Name Spoqa Han Sans. | `assets/fonts/licenses/SpoqaHanSans-OFL.txt` |

The SIL Open Font License, Version 1.1 applies to each family under the terms in the file named above; the full text is not repeated here.

## Legacy binary templates (Apache POI, Apache-2.0)

The `.doc` and `.ppt` writers (PD-090) start from two blank documents taken from Apache POI (https://github.com/apache/poi, Apache License 2.0; the full license text is reproduced in the generated section above):

- `empty.doc`: POI test data (`test-data/document/empty.doc`), SHA-256 `5f421a3970c70f12296073478380c0849a54c03fb2f4842392c7d3c5039bf1eb`.
- `empty.ppt`: POI's HSLF blank-presentation resource (`empty.ppt`), SHA-256 `12142803e51e4cff4804253f8e32857d312ddb4e8c78139bab99114062fbf210`.

Modifications: only the needed streams were kept (`WordDocument`, `1Table`, `PowerPoint Document`); `SummaryInformation`, `DocumentSummaryInformation`, `CompObj` and `Current User` were dropped; the author name, saved-by path, document timestamps and the layout-cache entries were removed or neutralised. The writer code is original and follows the MS-DOC and MS-PPT specifications, using POI's HWPF/HSLF structure as a reference.

| Extracted file | Size | SHA-256 |
|---|---|---|
| `crates/easyoffice-formats/assets/legacy/doc_sepx.bin` | 30 | `4eaa72fa1235e729083453782e21c11dcc81d4b46cf488612a4a7239c1a36807` |
| `crates/easyoffice-formats/assets/legacy/doc_table.bin` | 1125 | `39272573d77bf90100be06f600f0ea7f6589e895b7170a3a2701a54d0105d59e` |
| `crates/easyoffice-formats/assets/legacy/doc_word_head.bin` | 1024 | `d3daa2c563fa94c3b43d42a319cc593e2f590c2597420aeabb89d22cb3dfd1f1` |
| `crates/easyoffice-formats/assets/legacy/ppt_document.bin` | 3177 | `9b1f1a0e62d918f761b538f751b925fe624e1f875934c66e74f15cf7de78ab1d` |

POI NOTICE (as published with the Apache POI sources):

```
Apache POI
Copyright 2003-2026 The Apache Software Foundation

This product includes software developed at
The Apache Software Foundation (https://www.apache.org/).

This product contains parts that were originally based on software from BEA.
Copyright (c) 2000-2003, BEA Systems, <http://www.bea.com/> (dead link),
which was acquired by Oracle Corporation in 2008.
<http://www.oracle.com/us/corporate/Acquisitions/bea/index.html>
<https://en.wikipedia.org/wiki/BEA_Systems>
Note: The ASF Secretary has on hand a Software Grant Agreement (SGA) from
BEA Systems, Inc. dated 9 Sep 2003 for XMLBeans signed by their EVP/CFO.

This product contains W3C XML Schema documents. Copyright 2001-2003 (c)
World Wide Web Consortium (Massachusetts Institute of Technology, European
Research Consortium for Informatics and Mathematics, Keio University)

This product contains the chunks_parse_cmds.tbl file from the vsdump program.
Copyright (C) 2006-2007 Valek Filippov (frob@df.ru)

This product contains parts of the eID Applet project
<http://eid-applet.googlecode.com> and <https://github.com/e-Contract/eid-applet>.
Copyright (c) 2009-2018
FedICT (federal ICT department of Belgium), e-Contract.be BVBA (https://www.e-contract.be),
Bart Hanssens from FedICT

ExceptionUtils is derived from `scala.util.control.NonFatal` in scala-library
which was released under the Apache 2.0 license.

Copyright (c) 2002-2023 EPFL
Copyright (c) 2011-2023 Lightbend, Inc.

Scala includes software developed at
LAMP/EPFL (https://lamp.epfl.ch/) and
Lightbend, Inc. (https://www.lightbend.com/).
```

## Desktop shell framework (Tauri)

The desktop shell (`crates/easyoffice-desktop`) is built on `tauri` 2.x, `tauri-build` and the official plugins `tauri-plugin-dialog` and `tauri-plugin-single-instance` (`Apache-2.0 OR MIT`, Copyright (c) The Tauri Programme within The Commons Conservancy, <https://github.com/tauri-apps/tauri>), used unmodified. No code of these crates was copied. The license texts travel in their source packages.

## Desktop shell PDF export crates (searchable PDF, PD-101)

The desktop shell (`crates/easyoffice-desktop`, not the engine crates) depends on `svg2pdf` (rhwp's vendored copy, MIT OR Apache-2.0, `vendor/svg2pdf`, copied from rhwp `1a76570` `vendor/svg2pdf` with its patch notes `RHWP_PATCHES.md`), `usvg`, `fontdb`, `pdf-writer` and `ttf-parser` (already in the closure through usvg) to turn the Hangul editor's page SVGs into PDFs with embedded, searchable text. `crates/easyoffice-desktop/src/pdf_export.rs` adapts the page assembly and the nested-SVG picture resolver (`pdf_image_href_resolver`) of rhwp's PDF renderer (`src/renderer/pdf.rs`, rhwp 0.8.7 at `1a76570`) and of the reference app `apps/desktop/src-tauri/src/pdf_export.rs`, and `crates/easyoffice-desktop/src/pdf_fonts.rs` adapts rhwp's fallback-font index (`PdfFontFallbackIndex`, `src/renderer/pdf.rs` lines 569-753) and the reference app's bundled-font setup (all MIT, Copyright (c) 2025-2026 Edward Kim). PD-131 adds three more adaptations of the same rhwp PDF renderer (rhwp 0.8.7 at `1a76570`, MIT, Copyright (c) 2025-2026 Edward Kim): `crates/easyoffice-desktop/src/pdf_tounicode.rs` (`remap_pua_in_tounicode` and its helpers, `src/renderer/pdf.rs` lines 953-1046, which rewrites private-use characters in the `ToUnicode` maps in place, now limited to `begincmap`..`endcmap` and with bounded loops), `crates/easyoffice-desktop/src/pdf_pua.rs` (the text tables behind it: `text_surface_replacement` and `pua_to_display_text` in `src/renderer/composer.rs` lines 4416-4452, the verified-symbol table of `src/renderer/` lines 12-84 and `map_pua_bullet_char` in `src/renderer/layout/paragraph_layout.rs` lines 11018-11145, merged into one sorted table) and `crates/easyoffice-desktop/src/pdf_synthetic.rs` (`src/renderer/pdf_synthetic_bold.rs` and `src/renderer/pdf_synthetic_italic.rs`, both whole files, merged into one pass with one analysis parse, an element limit and a reduced analysis copy of the page). The shell now also names `roxmltree` (already used by usvg) as a direct dependency, and its tests `miniz_oxide` (already used by svg2pdf); both are in the closure counted below. Licenses of the 77 crates in the dependency closure of the four (checked with `cargo metadata`):

| License expression | Crates |
| --- | --- |
| `MIT OR Apache-2.0` | 37: arrayvec, base64, bitflags, cfg-if, crc32fast, data-url, euclid, fdeflate, flate2, font-types, hashbrown, itoa, libc, log, memmap2, num-traits, once_cell, pdf-writer, png, polycool, proc-macro2, quote, read-fonts, roxmltree, serde, serde_core, serde_derive, siphasher, skrifa, smallvec, subsetter, svg2pdf, syn, ttf-parser, unicode-bidi, unicode-script, write-fonts |
| `MIT` | 12: core_maths, float-cmp, fontconfig-parser, fontdb, imagesize, libm, pico-args, rgb, rustybuzz, simd-adler32, strict-num, xmlwriter |
| `Apache-2.0 OR MIT` | 9: equivalent, indexmap, kurbo, kurbo, resvg, rustc-hash, simplecss, svgtypes, usvg |
| `MIT/Apache-2.0` | 4: bitflags, unicode-bidi-mirroring, unicode-ccc, unicode-vo |
| `Zlib OR Apache-2.0 OR MIT` | 3: bytemuck, bytemuck_derive, tinyvec |
| `MIT OR Zlib OR Apache-2.0` | 2: miniz_oxide, miniz_oxide |
| `Zlib` | 2: slotmap, zlib-rs |
| `BSD-3-Clause` | 2: tiny-skia, tiny-skia-path |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 2: unicode-ident, unicode-properties |
| `0BSD OR MIT OR Apache-2.0` | 1: adler2 |
| `BSD-2-Clause` | 1: arrayref |
| `Unlicense OR MIT` | 1: memchr |
| `Apache-2.0 OR BSL-1.0` | 1: ryu |

No MPL, GPL or unlicensed crate is in the closure.

## Desktop shell update check crates (update notice, PD-103)

The desktop shell (`crates/easyoffice-desktop`, not the engine crates) depends on `ureq` 2.x (HTTP over the operating system's TLS stack through `native-tls`), `native-tls` and `open` to ask the public releases address for the newest release at most once a day and to open the release page in the default browser. Licenses of the 15 crates these three add to the shell (checked with `cargo metadata`; `open`, `is-docker` and `is-wsl` are MIT): `ureq`, `native-tls`, `openssl-probe`, `security-framework`, `security-framework-sys`, `foreign-types`, `foreign-types-shared`, `openssl-macros`, `base64`, `schannel` are MIT OR Apache-2.0 (or MIT/Apache-2.0, or MIT); `openssl` is Apache-2.0 and `openssl-sys` is MIT (Linux builds only; the OpenSSL library itself is the system's own and is not bundled). No bundled TLS library, no MPL, GPL or unlicensed crate.

## Desktop shell signed update crates (signed install, PD-103)

The desktop shell (`crates/easyoffice-desktop`, not the engine crates) depends on `tauri-plugin-updater` 2.x (Apache-2.0 OR MIT; built with `native-tls` and `system-proxy`, without the default `rustls-tls`, so the download uses the operating system's TLS stack like the release check above) and `futures-util` (already in the dependency closure) to download, verify with a minisign public key and install the new version. The plugin's JavaScript commands are registered but never granted to a window. Licenses of the crates the plugin adds to the lock file (checked with `cargo metadata`): `minisign-verify` and `tokio-native-tls` are MIT; `tauri-plugin-updater`, `zeroize`, `rustls-pki-types`, `tar`, `core-foundation` 0.9.4, `system-configuration`, `system-configuration-sys`, `windows-registry`, `xattr`, `osakit`, `filetime` and `hyper-tls` are MIT OR Apache-2.0 (or MIT/Apache-2.0); `objc2-osa-kit` is Zlib OR Apache-2.0 OR MIT. No MPL, GPL or unlicensed crate, no bundled TLS library.

## Desktop shell first-page preview crates (start-window cards, PD-142)

The desktop shell (`crates/easyoffice-desktop`, not the engine crates) depends on the rhwp crate of the pinned submodule `third_party/rhwp` (MIT, Copyright (c) 2025-2026 Edward Kim, see the rhwp section; consumed unmodified through a `[patch.crates-io]` path entry, without the `native-skia` feature) to lay out the first page of a Hangul document that has no preview picture; `usvg`, `resvg` and `tiny-skia` (already in the shell, PD-132) draw it. The lock file gains 93 packages (rhwp, its three workspace crates and their dependencies; checked with `cargo metadata --locked`, which also lists packages for platforms other than this one). Their licenses, by SPDX expression: `MIT OR Apache-2.0` (60 with the `Apache-2.0 OR MIT` spelling), `MIT` (12, among them `rhwp`, the `rhwp-*` crates and `cfb`), `MIT/Apache-2.0` (3), `MIT OR Apache-2.0 OR Zlib` (4, the `zune-*` crates), `BSD-3-Clause` (`curve25519-dalek`, `ed25519-dalek`, `subtle`), `BSD-3-Clause OR Apache-2.0` (`moxcms`, `pxfm`), `BSD-2-Clause OR Apache-2.0 OR MIT` (`zerocopy`, `zerocopy-derive`), and one each of `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` (`blake3`), `Unlicense OR MIT` (`byteorder-lite`), `CC0-1.0 OR MIT-0 OR Apache-2.0` (`constant_time_eq`), `(Apache-2.0 OR MIT) AND BSD-3-Clause` (`encoding_rs`), `MIT OR Apache-2.0 OR BSD-1-Clause` (`fiat-crypto`), `MIT OR Apache-2.0 OR WTFPL` (`pcx`) and `Apache-2.0` (`zopfli`). Every expression offers a permissive license; none is GPL, AGPL or LGPL.

## Desktop shell PDF editing crates (PD-170)

The desktop shell (`crates/easyoffice-desktop`, not the engine crates) depends on `hayro-syntax` 0.9.0 (reads a PDF file; requested without its image decoders, `default-features = false, features = ["std"]`, but PD-179 enables them through `hayro`) and `hayro-write` 0.9.0 (copies a page and the objects it uses into a new file), both `Apache-2.0 OR MIT`, Copyright (c) The Hayro Authors, <https://github.com/LaurenzV/hayro>; `hayro-syntax` is used unmodified, `hayro-write` is vendored in `vendor/hayro-write` (PD-266: all 684 lines of `src/lib.rs` and `src/primitive.rs` of 0.9.0 are copied, 39 lines are added and 3 are changed so that the `/OC` entries of copied objects are kept and an indirect object can be copied by request; `PATCHES.md` lists them, the license texts are copied with it); and on `pdf-writer` 0.15.0 (`MIT OR Apache-2.0`, the version `hayro-write` writes with; the shell's own `pdf-writer` 0.12 is unchanged) and `flate2` (already in the closure), to edit a PDF without losing its pictures, fonts and layout. They add three crates to the compiled closure (`hayro-syntax`, `hayro-write`, `pdf-writer` 0.15.0; `rustc-hash`, `smallvec`, `memchr`, `itoa`, `ryu`, `bitflags` and `flate2` were already in it, checked with `cargo tree -p easyoffice-desktop -e normal`). The lock file also lists the optional `hayro-ccitt`, `hayro-jbig2` and `hayro-jpeg2000` (`Apache-2.0 OR MIT`, Copyright (c) The Hayro Authors), which are not compiled. The worker process limits (PD-170) call `libc` (Unix) and `windows-sys` 0.61 (Windows), both already in the closure through the Tauri stack. No code of `hayro-syntax`, `pdf-writer` and `flate2` was copied. `hayro-write` is copied whole into `vendor/hayro-write` with the patch described above (PD-266), so its 684 source lines are in this repository; the license texts of the other crates travel in their source packages and those of `hayro-write` in its copied folder. Every expression offers a permissive license; none is GPL, AGPL or LGPL.

## Desktop shell PDF first-page crates (start-window cards, PD-179)

The desktop shell (`crates/easyoffice-desktop`, not the engine crates) depends on `hayro` 0.9.0 (rasterizes the first page of a PDF for the start-window card, in the PDF worker process of PD-170), `Apache-2.0 OR MIT`, Copyright (c) The Hayro Authors, <https://github.com/LaurenzV/hayro>, used unmodified, with its default features `embed-fonts` and `embed-cmaps`. `hayro` enables the image decoders and the `unsafe` feature of `hayro-syntax` (`hayro-jbig2`, `hayro-jpeg2000`, `hayro-ccitt`, `zune-jpeg`, `zlib-rs` through `flate2`), which the PD-170 section above lists as not compiled; the edit path still never draws a page. The desktop shell also names `easyoffice-sheet` (this repository) as a direct dependency. 22 crate versions are new in the compiled closure (checked with `cargo tree -p easyoffice-desktop -e normal` against the previous lock file): `hayro`, `hayro-interpret`, `hayro-font`, `hayro-cmap`, `hayro-postscript`, `vello_cpu`, `vello_common`, `peniko`, `color`, `linebender_resource_handle`, `guillotiere`, `fearless_simd` (0.7.0 and 1.2.0), `pic-scale`, `ab_glyph_rasterizer`, `font-types`, `read-fonts` 0.46.0, `skrifa` 0.49.0, `brotli`, `brotli-decompressor`, `alloc-stdlib` and `alloc-no-stdlib`. Licenses by SPDX expression: `Apache-2.0 OR MIT` (the `hayro-*` crates, `vello_*`, `peniko`, `color`, `linebender_resource_handle`, `fearless_simd`), `MIT OR Apache-2.0` (`font-types`, `read-fonts`, `skrifa`, `guillotiere`), `BSD-3-Clause OR Apache-2.0` (`pic-scale`), `Apache-2.0` (`ab_glyph_rasterizer`), `BSD-3-Clause AND MIT` (`brotli`), `BSD-3-Clause/MIT` (`brotli-decompressor`) and `BSD-3-Clause` (`alloc-stdlib`, `alloc-no-stdlib`). Every expression offers a permissive license; none is GPL, AGPL or LGPL. No code of these crates was copied; the license texts travel in their source packages.

`hayro-font` embeds the 14 standard-font substitutes (Foxit PFB fonts extracted from PDFium) and `hayro-cmap` the predefined CMaps of the PDF specification (Adobe). Both are redistributed in the shell binary under BSD-3-Clause, and their notices are reproduced here.

```
Copyright 2014 PDFium Authors. All rights reserved.
Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.
   * Neither the name of Google Inc. nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

```
Copyright 1990-2023 Adobe. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

Redistributions of source code must retain the above copyright notice,
this list of conditions and the following disclaimer.

Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

Neither the name of Adobe nor the names of its contributors may be
used to endorse or promote products derived from this software without
specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## Reference app (code adapted)

The reference app (MIT, reference commit `80888af`, v0.4.5) is the model for the EasyOffice window UI (PD-098) and for the build and verification patterns of the `apps/hwp-studio` overlay (pinned upstream checkout, Vite alias overlay with an override manifest, vendored WebAssembly with provenance, font policy). Where the reference app code is ported, the TypeScript is rewritten as plain JavaScript or the CSS is copied with its tokens renamed, each ported file starts with a header naming its reference app source, and the file is listed here first:

| EasyOffice file | Reference app source | Change |
| --- | --- | --- |
| `crates/easyoffice-desktop/src/pdf_font_fallbacks.rs` | `apps/desktop/src-tauri/src/pdf_font_fallbacks.rs` | Rewritten around one sorted static table of document font names with an allocation-free binary search; fallback chains name the platform's installed Korean fonts |
| `crates/easyoffice-desktop/src/pdf_export.rs` | `apps/desktop/src-tauri/src/pdf_export.rs` | Page SVGs come from the editor as strings and are converted with svg2pdf directly (no native rhwp core); limits and fail-closed pages added; pages are converted 16 at a time on up to 4 threads and renumbered straight into the output; a page whose fonts cannot be subset is retried with outlined text and fails the export when it still fails (the reference app skips the page) |
| `crates/easyoffice-desktop/src/pdf_job.rs`, `crates/easyoffice-app/ui/hwp-session.js` (`exportPdf`), `crates/easyoffice-desktop/src/native_menu.rs` (`file:export-pdf`, `file:print`) | `apps/studio-host/src/core/tauri-bridge.ts` (lines 226-245), `apps/desktop/src-tauri/src/commands.rs` (lines 440-482), `apps/desktop/src-tauri/src/menu.rs` (lines 27-32, 88-89) | The save location is chosen first and the finished file is opened with `open::that` (`openAfter`); the export is three commands instead of one call, pages travel ten at a time and the file is written once at the end, no staged copy of the document passes through a file; the file is opened only when it is named `.pdf`; the file menu ends in `PDF로 내보내기…` (CmdOrCtrl+E) and `인쇄…` (CmdOrCtrl+P) as in the reference app |
| `crates/easyoffice-desktop/src/pdf_fonts.rs` | `apps/desktop/src-tauri/src/font_catalog.rs` (`pdf_font_dirs`) and rhwp `src/renderer/pdf.rs` (`PdfFontFallbackIndex`) | Bundled Noto Sans KR folder read before the system fonts and mapped generic families as in the reference app; the fallback index keeps two bits per (character, face) instead of an `Option<bool>` slot, drops the database-identity checks (one index per export over one database) and the face lookup uses `ttf-parser` directly; font files are mapped for the length of one export |
| `crates/easyoffice-desktop/src/font_catalog.rs` (with `font_catalog_tests.rs`) | `apps/desktop/src-tauri/src/font_catalog.rs` (`LocalFontEntry`, `collect_local_font_entries`, `read_desktop_local_font`) and `apps/desktop/src-tauri/src/commands.rs` (lines 339-349, `list_local_fonts` / `read_local_font`) | Same list (family, style, weight, localized family names as aliases) and the same supported font extensions; the list comes from the PDF export's font database instead of a second scan; a face is an opaque number valid only with the window's list revision (no path crosses to the page); a read is bounded (64 MiB per file, font signature, 512 MiB per window, 25,000 faces) and returns binary instead of a number array |
| `apps/hwp-studio/src/embed/host-fonts.ts`, `crates/easyoffice-app/ui/hwp-host.js` (font channel), `crates/easyoffice-app/ui/desktop-bridge.js` (`systemFonts`) | `apps/studio-host/src/core/desktop-local-fonts.ts` (`detectDesktopFontEntries`, `readDesktopFontBytes`, `fontBinaryCache`) | TypeScript rewritten as JavaScript and as a host-font provider of the editor; the list is asked once, a read is cached by file so the faces of a collection read it once, and the sandboxed editor frame asks over the embed port while only the shell page calls the commands |
| `crates/easyoffice-desktop/src/native_menu.rs` | `apps/desktop/src-tauri/src/menu.rs` | Menu layout and "menu id to the focused window" delivery kept, installed on macOS only (as in the reference app; Windows and Linux draw only the page's menu row); one static item table that holds every item of the page's menu bar, system items for editing, a custom quit item with `CmdOrCtrl+Q` at the end of the application menu, check items for the theme and the update switch, Korean labels, delivery by evaluating a fixed script (no event permission) |
| `crates/easyoffice-desktop/src/app_quit.rs` | `apps/desktop/src-tauri/src/app_quit.rs` | The same queue of window labels (focused window first) advanced as windows are destroyed and ended by cancel; each window is asked with `close()` so the existing close confirmation runs, the page ends the quit through a narrow command, used on every OS (macOS: the application menu's quit item and the Dock; Windows and Linux: `Ctrl+Q`) |
| `crates/easyoffice-app/ui/workspace-session.js` (right-click guard, shortcut labels) | `apps/studio-host/src/core/desktop-chrome.ts` | TypeScript rewritten as JavaScript; the same chrome parts block the web view's context menu (text fields keep theirs), menu shortcut labels are written at start-up through the catalog's one formatter |
| `crates/easyoffice-app/ui/ribbon-catalog.js` (`formatPlatformShortcut`, macOS side) | `apps/studio-host/src/core/platform.ts` (`normalizeShortcutLabel`) | Same macOS glyph notation, with the modifiers in the macOS order; the Windows side was already there |
| `crates/easyoffice-desktop/src/linux_runtime.rs` (with its `linux_runtime_tests`) | `apps/desktop/src-tauri/src/linux_runtime.rs` (and its tests) | Ported whole: WebKitGTK fallback variables on some distributions (set only when the user has not set them) and the AppImage GTK input-method cache correction; the tests drop the `unsafe` blocks (edition 2021) and the module is built on other OSes only for those tests |
| `apps/hwp-studio/src/embed/desktop-chrome.ts` | `apps/studio-host/src/core/desktop-chrome.ts` and `platform.ts` (`normalizeShortcutLabel`) | Mac modifier symbols for the editor frame's static chrome (split-menu text and tooltips) and the same context-menu guard on the menu bar, toolbars and status bar; written as a self-contained module of the editor overlay |
| `crates/easyoffice-app/ui/file-open.js` (drop handling) | `apps/studio-host/src/main.ts` (drag and drop) | Browser drop behaviour is off for the whole document; the editing area is the only drop target and shows the outline |
| `crates/easyoffice-app/ui/update-notice.js` | `apps/studio-host/src/ui/update-notice.ts` | TypeScript rewritten as JavaScript; the available, downloading and error states with the reference app's progress text, one "install and restart" action (the reference app's install and restart actions in one), a release page button and an "installed, restart held for unsaved changes" state added, the page polls the shell instead of receiving an event, card in the bottom left corner |
| `crates/easyoffice-app/ui/home.css` (update notice rules) | `apps/studio-host/src/styles/update-notice.css` | Same rules and class names, rhwp tokens, bottom left position |
| `crates/easyoffice-desktop/src/update_check.rs` | `apps/desktop/src-tauri/src/updates.rs` (idea only) | A once-a-day release check in Rust (GitHub releases API, switch, 404 as "no release") that shows the card; the signed install is `update_install.rs` |
| `crates/easyoffice-desktop/src/update_install.rs` | `apps/desktop/src-tauri/src/updates.rs` | Check, download with progress, refusal while a document is unsaved (`has_dirty_documents`), install and restart on `tauri-plugin-updater`, an error keeps the update for a retry. State kept in the update service and polled by the page instead of an event; the announced version and the download address are checked against the notice before anything is installed, the package size and the time are limited, the unsaved check is repeated after the download and before the restart, the install runs on a blocking worker, and the pipeline is generic over the package so the fail-closed rules are unit tested |
| `crates/easyoffice-app/ui/home-screen.js` | `apps/studio-host/src/ui/home-screen.ts` | TypeScript rewritten as JavaScript; six new-document buttons with icons, cards without paths, opening through the window bridge; `loadRecentPreview` is rewritten so the picture arrives as Base64 from the shell (the picture stored in the file, not a rendered page) and is drawn into a canvas, one card after the other |
| `crates/easyoffice-app/ui/recent-documents-dialog.js` | `apps/studio-host/src/ui/recent-documents-dialog.ts` | TypeScript rewritten as JavaScript; entries are `{ id, name, kind }`, no clear button |
| `crates/easyoffice-app/ui/custom-select.js` | `apps/studio-host/src/ui/custom-select.ts` | TypeScript rewritten as JavaScript; the same selector list (`sb-combo`, `sb-ls-select`, `dialog-select`, `formula-select`), programmatic `value` assignments refresh the label |
| `crates/easyoffice-app/ui/color-picker.js` | `apps/studio-host/src/ui/color-picker.ts`, `apps/studio-host/src/ui/toolbar-popup.ts` | TypeScript rewritten as JavaScript; palette, recent colours and popup behaviour as in the reference, own storage key, no stale-document guard |
| `crates/easyoffice-app/ui/toolbar.css` (combo box rules) | `apps/studio-host/src/styles/custom-select.css` | Generic `.custom-select*` rules with EasyOffice widths for the slide toolbar |
| `crates/easyoffice-app/ui/toolbar.css` (colour picker rules) | `apps/studio-host/src/styles/color-picker.css` | Palette, swatch and recent-colour rules; the toggle arrow and host are EasyOffice's |
| `crates/easyoffice-app/ui/home.css` | `apps/studio-host/src/styles/home-screen.css`, `recent-documents-dialog.css` | Same rules and class names; flex child instead of an absolute layer, action icons, modal base rules from rhwp-studio `dialogs.css` |
| `crates/easyoffice-app/ui/hwp-print.js` | `apps/studio-host/src/ui/print-dialog.ts` | TypeScript rewritten as JavaScript; same hidden print root, one `@page` size per paper size and progress text, SVG sanitizing kept; pages come from the editor in batches of ten over the embed port, a cancel button and a text budget are added, page ids are namespaced per page |
| `crates/easyoffice-app/ui/office-print.js` | `apps/studio-host/src/ui/print-dialog.ts` | TypeScript rewritten as JavaScript; same hidden print root, `@page` rules, shell print command and `afterprint` cleanup, with the pages built from the document editor, the sheet range or the slide deck instead of editor SVG |
| `crates/easyoffice-app/ui/shell.css` (find dialog layout) | `rhwp-studio/src/styles/find-dialog.css` | Same placement, 70px label column, right-aligned status line and button rows (find, then replace); EasyOffice tool-dialog classes and tokens instead of the editor's
| `crates/easyoffice-app/ui/shell-chrome.js` (`enableDialogKeys`, `enableDialogDrag`), `shell.css`, `home.css` (dialog rows, fields, buttons) | `rhwp-studio/src/ui/dialog.ts`, `dialog-drag.ts`, `src/styles/dialogs.css` | Same key handling while a modal dialog is open (Esc, Enter outside a text field, other keys kept inside), first focus on the primary button, no close on backdrop click, same dialog row, label, field, checkbox and button values; applied to native `<dialog>` elements |
| `crates/easyoffice-desktop/src/print_webview.rs` | `apps/desktop/src-tauri/src/commands.rs` (`print_webview`, lines 308-312) | The same command: print the calling window's webview with `WebviewWindow::print`; the error text sent to the page is fixed and the system error goes to the log |
| `crates/easyoffice-app/ui/shell.css` (print rules) | `apps/studio-host/src/ui/print-dialog.ts` (inline print style) | Same print rules moved into a static stylesheet; the per-size rules are added through a constructed stylesheet |
| `apps/hwp-studio/src/ui/toolbar-labels.ts` | `apps/studio-host/src/host/toolbar-labels.ts` | Same storage rule and menu check state; the key is `toolbar-labels`, and the module can run against a fake page for tests |
| `apps/hwp-studio/src/command/cell-selection.ts`, `crates/easyoffice-app/ui/shell-chrome.js` (`EMBED_EDITOR_COMMANDS`), `crates/easyoffice-desktop/src/native_menu.rs` (표·보기 items) | `apps/desktop/src-tauri/src/menu.rs`, `apps/studio-host/src/core/desktop-events.ts` | Same menu entries (Select Cells with `CmdOrCtrl+Alt+T`, Merge Cells, Split Cells, Zoom In/Out, Actual Size, Fit Page, Fit Width) and the same command ids; the event goes over the embed port instead of a window event |
| `apps/hwp-studio/src/eo-overrides.css` (toolbar label rules) | `apps/studio-host/src/styles/toolbar-labels.css` | Same rules |
| `crates/easyoffice-desktop/src/os_recent.rs` | `apps/desktop/src-tauri/src/macos_recent_documents.rs` | Same calls; errors ignored, no-op off macOS |
| `crates/easyoffice-desktop/src/tracked_files.rs` (fingerprint) | `apps/desktop/src-tauri/src/state.rs` (lines 643-690) | Length, modification time and a streamed content hash; 64-bit SipHash-1-3 from the standard library instead of 32-bit FNV-1a, a read cap, fingerprint from bytes the caller already holds |
| `crates/easyoffice-desktop/src/save_file.rs` (write) | `apps/desktop/src-tauri/src/state.rs` (lines 621-640) | Temporary file next to the target, `sync_all`, rename; the temporary file is created by hand (`create_new`, process id, counter, clock) so the target's permission bits are kept |
| `crates/easyoffice-desktop/src/save_verify.rs` | `apps/desktop/src-tauri/src/state.rs` (lines 189-215) | Bytes are checked before they replace a file; the engine's ZIP and OLE2 structure parsers stand in for the native Hangul core, which this shell does not have |
| `scripts/update-rhwp.mjs` | `scripts/update-rhwp-upstream.mjs` | Explicit tag only (`vX.Y.Z`), resolved to a commit on the remote; new upstream files are read from that commit without a checkout; a changed override baseline fails until `--accept-overrides`; plan first, writes last, pin files and gitlink restored on failure; the vendored WASM is not rebuilt; `--dry-run` |
| `.github/workflows/release.yml` | the reference app's desktop release workflow (`.github/workflows/*-desktop.yml`) | Linux build matrix (x64 on `ubuntu-22.04` for AppImage, deb and rpm; arm64 on `ubuntu-22.04-arm` for deb), fixed release asset names, one combined checksum file and the draft release; the version, tag and draft state are resolved once in a `prepare` job; signed updater artifacts (`createUpdaterArtifacts` in the build config, the signing key from the repository secrets and required by every run, `latest.json` written in the publish job); no notarization, staging and checks are scripts below |
| `.github/actions/install-linux-deps/action.yml` | the reference app's desktop release workflow (`.github/workflows/*-desktop.yml`), step "Install Linux dependencies" | Same package list (WebKitGTK 4.1, GTK 3, libxdo, FUSE 2, patchelf, rpm); a composite action shared by the CI job and the release job, Ubuntu 22.04 only so no FUSE package name fallback |
| `scripts/release-assets.mjs` | the reference app's desktop release workflow (`.github/workflows/*-desktop.yml`), functions `copy_release_asset` and `copy_updater_asset`, the `latest.json` assembly and the checksum step | One bundle per extension copied to a fixed asset name, its `.sig` copied next to it and one `latest.json` entry per updater platform key (`url`, `signature`), as a script with a unit test; a missing or ambiguous bundle or signature fails, the signatures must carry the release version, the URLs name the release tag, the checksum file is written only for exactly the release files, and the README download names are tested against it |
| `scripts/verify-linux-bundles.mjs` | the reference app's desktop release workflow (`.github/workflows/*-desktop.yml`), step "Verify Linux ABI baseline" | The AppImage is extracted with `--appimage-extract` and never run; deb and rpm metadata, the desktop entry and the license notices are checked as well; the glibc scan is not taken over (the runner image fixes the baseline); with `--signed` every package must have a signature file |
| `apps/quicklook/Sources/Preview/HwpPreviewProvider.swift`, `apps/quicklook/Sources/Thumbnail/HwpThumbnailProvider.swift`, `apps/quicklook/Sources/Shared/EasyOfficeQuickLookFFI.swift` and `.h`, `apps/quicklook/Resources/` | `apps/desktop/quicklook/Sources/` and `Resources/` | Same structure (a data-based preview that answers with a PDF, a thumbnail provider that prefers the picture stored in the file and otherwise draws the first page, the shared file reader with its 50 MiB limit, the entitlements of an app sandbox with read access to the opened file); names and bundle identifiers are EasyOffice's, supported types are the app's own exported UTIs, a failure is thrown or answered with no thumbnail so the system shows its default icon (the reference app draws a stand-in or a text message), file names are not logged |
| `apps/quicklook/rust/src/` | `apps/desktop/quicklook/rust/src/lib.rs`, `pdf.rs`, `tests.rs` | The C ABI and the multi-page PDF assembly (chunks renumbered into one object space) are taken over; the pages go from rhwp's page SVG to `usvg` and `svg2pdf` directly and to PNG with `resvg` (the reference app uses Skia and rasterizes before the PDF), and every call runs on a worker thread under a deadline with input, SVG, pixel, page and PDF limits and a cap on abandoned workers |
| `scripts/build-quicklook-macos.mjs`, `scripts/run-quicklook-tests.mjs` | `scripts/build-quicklook-macos.mjs`, `scripts/test-quicklook-macos.mjs` | Rust static library, then `swiftc` per extension, then `codesign` with the entitlements; universal (both architectures joined with `lipo`), `-dead_strip`, the version written into the extension's Info.plist, a table shared with the verifier and the tests |
| `apps/hwp-studio/src/command/clipboard-cut.ts`, `apps/hwp-studio/src/command/safe-cut.ts` | `apps/studio-host/src/command/commands/edit.ts` (lines 20-150) | The cut (`edit:cut`) and copy (`edit:copy`, `safeCopyCommand`) commands copy the text selection into the editor's clipboard, write it to the system clipboard through `writeTextHtmlWithPlainFallback` (falling back to plain text when `ClipboardItem` is unavailable or rejected) and the cut command deletes the selection only after the write was accepted and the selection is unchanged; a selection in a nested table is copied by its cell path as the editor's copy event does; the clipboard write is injected so the order can be tested without a browser; object, header and footer, footnote, cell-block and form-mode cuts and copies keep the editor's own command |
| `apps/hwp-studio/src/ui/toolbar.ts`, `apps/hwp-studio/src/ui/custom-select.ts`, `apps/hwp-studio/src/ui/color-picker.ts`, `apps/hwp-studio/src/ui/toolbar-popup.ts`, `apps/hwp-studio/src/core/font-authoring-policy.ts`, `apps/hwp-studio/src/styles/custom-select.css`, `apps/hwp-studio/src/styles/color-picker.css`, `apps/hwp-studio/src/styles/font-set-dialog.css` | `apps/studio-host/src/ui/toolbar.ts`, `custom-select.ts`, `color-picker.ts`, `toolbar-popup.ts`, `apps/studio-host/src/core/font-authoring-policy.ts` and `apps/studio-host/src/styles/custom-select.css`, `color-picker.css`, `font-set-dialog.css` | Cross-platform custom combo boxes, shared colour-picker palette with recent text colours, authoring font fallback policy (rewritten with bit-packed ASCII/UTF-16 word scanning) and font-set dialog rules; class and storage prefixes use `eo` |

Colour and theme code taken from rhwp-studio is listed in the rhwp section.

Used under the MIT license:

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

## Quick Look extension crates (macOS preview and thumbnail, PD-140)

`apps/quicklook/rust` (its own Cargo workspace, linked into the two `.appex` bundles as a static library) depends on the rhwp crate of the pinned submodule `third_party/rhwp` (MIT, Copyright (c) 2025-2026 Edward Kim, see the rhwp section; consumed unmodified, without the `native-skia` feature), on `usvg`, `resvg`, `tiny-skia`, `svg2pdf` (rhwp's vendored copy, `vendor/svg2pdf`) and `pdf-writer` (the crates of the desktop shell section above; `resvg` and `tiny-skia` are the versions that draw the preview picture of saved files, PD-132), and on `crates/easyoffice-desktop/src/pdf_font_fallbacks.rs`, which it includes as a module. 187 crates are in the closure (`cargo metadata --locked` for `aarch64-apple-darwin`, normal dependencies, Rust 1.98). Their licenses, by SPDX expression: `MIT OR Apache-2.0` (123 including the `Apache-2.0 OR MIT` spelling), `MIT` (25 plus 5 spelled `MIT/Apache-2.0`), `BSD-3-Clause` (`curve25519-dalek`, `ed25519-dalek`, `subtle`, `tiny-skia`, `tiny-skia-path`), `Zlib` or `Zlib OR Apache-2.0 OR MIT` (11 with `zune-core`, `zune-jpeg`, `bytemuck`, `tinyvec`, `miniz_oxide`, `slotmap`, `zlib-rs`), `Unlicense OR MIT` (`byteorder`, `byteorder-lite`, `memchr`), `BSD-3-Clause OR Apache-2.0` (`moxcms`, `pxfm`), `BSD-2-Clause` (`arrayref`) or `BSD-2-Clause OR Apache-2.0 OR MIT` (`zerocopy`), `(MIT OR Apache-2.0) AND Unicode-3.0` (`unicode-ident`, `unicode-properties`), `(Apache-2.0 OR MIT) AND BSD-3-Clause` (`encoding_rs`), `Apache-2.0` (`zopfli`), and one each of `0BSD OR MIT OR Apache-2.0` (`adler2`), `Apache-2.0 OR BSL-1.0` (`ryu`), `MIT OR Apache-2.0 OR WTFPL` (`pcx`), `CC0-1.0 OR MIT-0 OR Apache-2.0` (`constant_time_eq`), `CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception` (`blake3`) and `Apache-2.0 / MIT` (`fnv`). Every expression offers a permissive license; none is GPL, AGPL or LGPL. The license texts of these crates travel in their source packages, as for the crates counted above.

## Engine code adapted from open-source Rust crates

Since PD-269 the engine crates link maintained crates under permissive licences instead of re-implementing them. `crates/easyoffice-formats` depends on `adler2` (`0BSD OR MIT OR Apache-2.0`), `base64` (`MIT OR Apache-2.0`), `crc32fast` (`MIT OR Apache-2.0`), `encoding_rs` (`(Apache-2.0 OR MIT) AND BSD-3-Clause`), `miniz_oxide` (`MIT OR Zlib OR Apache-2.0`), `pdf-writer` 0.12 (`MIT OR Apache-2.0`, PD-269 step 4: the PDF dictionaries and cross-reference table of `pdf_writer.rs`, `pdf_image.rs` and `pdf_highlight.rs`; the version the desktop shell links, so the lock file holds one copy), `quick-xml` (`MIT`), `serde` and `serde_json` (`MIT OR Apache-2.0`), `sha1` and `sha2` (`MIT OR Apache-2.0`) and `zip` (`MIT`, no compression feature on); their transitive crates are the ones counted in the sections above and below (`cargo metadata --locked`). No code of these crates is copied; the licence texts travel in their source packages, and the texts of the three notices that name a copyright holder are kept below. Code that is still ported or re-implemented from another project is listed after them.

### base64 (MIT OR Apache-2.0)

Source: https://github.com/marshallpierce/rust-base64 (v0.23). Linked by `crates/easyoffice-formats/src/base64_codec.rs`, which only fixes the padding, whitespace and empty-input contract. Used under the MIT license:

```
The MIT License (MIT)

Copyright (c) 2015 Alice Maz

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

### miniz_oxide (MIT OR Zlib OR Apache-2.0, used under MIT)

Source: https://github.com/Frommi/miniz_oxide (version 0.9.1; licensed `MIT OR Zlib OR Apache-2.0`, used here under MIT). Linked by `crates/easyoffice-formats/src/deflate.rs` for raw DEFLATE and zlib compression and decompression; the output budget and the zlib trailer rules around it are the engine's own.

```
MIT License

Copyright 2013-2014 RAD Game Tools and Valve Software
Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC
Copyright (c) 2017 Frommi
Copyright (c) 2017-2024 oyvindln

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

### encoding_rs (EUC-KR decoder and legacy code page tables, (Apache-2.0 OR MIT) AND BSD-3-Clause)

Source: https://github.com/hsivonen/encoding_rs (v0.8.44; code `Apache-2.0 OR MIT`, Copyright Mozilla Foundation, table data BSD-3-Clause from the WHATWG Encoding Standard).
`crates/easyoffice-formats/src/cp949.rs` links the crate's `EUC_KR` decoder (code page 949) and keeps only the three decode policies of the readers.

`crates/easyoffice-formats/src/code_pages.rs` still reads four tables in `crates/easyoffice-formats/src/code_page_data/` (`sbcs.bin`, `sjis.bin`, `gbk.bin`, `big5.bin`): for the single-byte code pages 874 and 1250..=1258 (the `index-windows-*` tables) and, for every lead and trail byte pair of Shift_JIS, GBK and Big5 (the `index-jis0208`, `index-gb18030` and `index-big5` tables), the character that v0.8.44 of the crate returns, written as little-endian `u16` (cells that are unassigned or lie beyond U+FFFF are 0). Moving these pages onto the crate is the next step of the replacement order of PD-269. The table data notice:

```
Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### FormulaJS (MIT)

Source: https://github.com/formulajs/formulajs (`@formulajs/formulajs`).
`crates/easyoffice-sheet/src/functions_complex.rs`, `crates/easyoffice-sheet/src/functions_date_fin.rs`, `crates/easyoffice-sheet/src/functions_eng.rs`, and `crates/easyoffice-sheet/src/functions_fin.rs` implement spreadsheet mathematical, engineering, statistical, date, financial, and complex-number semantics in safe Rust following the FormulaJS reference specifications.

```
The MIT License (MIT)

Copyright (c) Sutoiku, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

### pulldown-cmark (MIT)

Source: https://github.com/pulldown-cmark/pulldown-cmark.
`crates/easyoffice-formats/src/rich_read_text.rs` adapts CommonMark/GFM block and inline scanning rules (thematic breaks, Setext headings, task lists, pipe tables, blockquotes) in safe Rust.

```
The MIT License (MIT)

Copyright 2015 Google Inc. All rights reserved.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```


### PDF standard font metrics (Helvetica widths)

`crates/easyoffice-formats/src/pdf_text_wrap.rs` holds the glyph advance widths of the PDF base-14 Helvetica font for ASCII codes 32 to 126 (thousandths of the font size), as published in the Core 14 AFM font metrics files (Copyright (c) 1985, 1987, 1989, 1990, 1997 Adobe Systems Incorporated). The metrics notice allows use, copying and distribution for any purpose without charge as long as the copyright notices are kept; only the numbers are used, no font program or AFM file is included.

### SheetJS SSF and IronCalc number formatter (Apache-2.0; MIT OR Apache-2.0)

Sources: https://github.com/SheetJS/sheetjs (`packages/ssf/ssf.js`, Apache License 2.0, Copyright (C) 2012-present SheetJS LLC) and https://github.com/ironcalc/IronCalc (`base/src/formatter`, used under the MIT license). `crates/easyoffice-sheet/src/text_format*` re-implements, in safe Rust, the SSF section choice with `[>100]`-style conditions and its continued-fraction search for `# ?/?` codes, and follows IronCalc's split of a format code into sections and tokens. No locale data is taken from either project; the Korean month, weekday and AM/PM names and the `DBNum` glyph tables are written directly. `crates/easyoffice-formats/src/sheet_numfmt.rs` takes the list of built-in format ids and their codes (ECMA-376 18.8.30, the SSF `table`) for reading and writing XLSX `numFmtId`. The full Apache License 2.0 text is reproduced in the generated section above. Number format in every output (PD-235): `crates/easyoffice-sheet/src/text_format/num.rs` `best_fraction` ports SSF `frac` (`ssf.js` lines 129-146, the convergent loop with the `0.00000005` stop and the fallback to the previous convergent when the denominator passes the bound; the earlier extension with the closest semiconvergent is removed because the program's own output does not use it), and the scientific exponent rule (a multiple of the integer placeholder count, `ssf.js` `write_num_exp` lines 351-376) is written over the 15-digit decimal text of the value instead of a floating-point division. Test vectors: `crates/easyoffice-sheet/tests/data/ssf/` holds SSF `test/comma.tsv` and `exp.tsv` unchanged, every 997th row of `dates.tsv` and every 4093rd row of `times.tsv`, and `fraction.json`, `implied.json`, `oddities.json` and `is_date.json` flattened to tab separated rows (Apache License 2.0, Copyright (C) 2012-present SheetJS LLC, https://github.com/SheetJS/ssf commit 0df2ab980280401609eb6ea6b67c70d792ad73ca, the project of the same name that kept the vectors), and `en_examples.tsv` holds the English-locale cases of IronCalc `base/src/formatter/test/test_en_examples.rs`, `test_general.rs`, `test_scientific.rs` and `test_time.rs` flattened the same way (MIT OR Apache-2.0, Copyright (c) IronCalc contributors, commit 53c00566515e3d67bf9b7bd2205db16771ea66e8). `crates/easyoffice-sheet/tests/data/xls_formats/` holds three spreadsheet files of the Apache POI test data (`test-data/spreadsheet/DateFormats.xls`, `FormatKM.xls`, `SimpleWithDataFormat.xls`, commit 47dcff769bb395034dddfd59c4609659b6dbc6a4, Apache License 2.0, Copyright The Apache Software Foundation, unchanged, SHA-256 `f3ac7e6f0f23e638cfeb0d34543e18f3b07cd93edd1e7a71be907c73a767310c`, `dafc1b91557f2abe8baecf499bfd21f03aa35248761a945f508bff4ec083db82` and `46f9eb234edc8c3803742415ad3e6906ec96f3de3de299fdd2ac4202f0652413`; the NOTICE text of that project is reproduced in the legacy templates entry above). `crates/easyoffice-formats/src/xls_styles.rs` reads the `FORMAT` record and the format index of the `XF` record following the BIFF8 layout in MS-XLS and the record reading of POI `FormatRecord` and `ExtendedFormatRecord`; no code is taken.

### Univer sheets conditional formatting (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets-conditional-formatting/src/models/calculate-unit-v2/*.ts`, `base/const.ts`, `models/type.ts`; commit a57b187e4540003489555ffca5774c2735006c19, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/rules/cf.rs` ports, function by function and in safe Rust, the cell value comparison (`compareWithNumber`), the top/bottom rank cut-off, the average and duplicate/unique rules, the colour scale interpolation (`getColorScaleFromValue`), the data bar geometry and the icon set thresholds. Modifications: text rules ignore case, the maximum of an all-negative range is its real maximum, equality uses a relative tolerance, icon sets follow the file format's thresholds (`>=`, lowest icon first), the average rule supports standard deviations, and statistics are computed once per range list and shared (`rules/data.rs`). The reference's formula service, time period rules, rendering and rule storage are not used.

### Univer sheets find and replace (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets-find-replace/src/controllers/sheet-find-replace.controller.ts` lines 1088-1157, `packages/find-replace/src/services/find-replace.service.ts` lines 507-518; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-app/ui/find-core.js` and `find-sheet.js` port the whole-cell comparison (`matchCellData`), the value-or-formula cell test (`hitCell`) and the find-by and find-scope options (`FindBy`, `FindScope`). Modifications: the whole-cell test is an anchored pattern on the shared matcher, a replacement edits the formula text only when searching by formula, and the scope covers the sheets of the open workbook. The reference's match model, cell rendering and selection handling are not used.

### Univer document hyperlinks (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/docs-hyper-link-ui/src/commands/commands/add-link.command.ts` lines 1-58, `update-link.command.ts` lines 1-90, `delete-link.command.ts` lines 1-45; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-app/ui/doc-link.js` ports the three commands: wrap the selection in a link that holds the address, replace the selected text by the label while keeping the first run's style and set a new address, and drop the link while keeping its text. Modifications: the commands run on the editor's DOM (an `a` element around the runs) instead of an index-based document body, the address is limited to `http`, `https` and `mailto`, and a link over several paragraphs or over another link is refused. The reference's popup, selection controller and rendering are not used.

### Univer document table of contents (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/docs-toc/src/commands/table-of-contents.command.ts` lines 120-190 (`updateTableOfContents`), 290-305 (`buildTableOfContentsInstruction`), 441-561 (`buildTableOfContentsBody`), 592-637 (`collectHeadings`, `getOutlineLevel`); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-app/ui/doc-toc.js` ports the heading collection (document order, blank titles and headings outside the block or the level range skipped), the level range of the block, one entry per heading at the heading's own level with an indent that grows with the level, the kept title text, and the in-place replacement that makes a second update a no-op. Modifications: levels 1 to 3 are offered (the reference takes 1 to 9), headings of levels 1 to 6 are collected, there are no page-number fields (the engine has no pagination), entries are links to heading ids, the block holds at most 5,000 headings and 500 characters per title, and the saved block is a plain paragraph list the DOCX and ODT writers wrap (`rich_toc_writers.rs`). The reference's index-based document model, field and range types, layout skeleton and history commands are not used.

### Univer sheets number formats (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets-numfmt/src/utils/decimal.ts` lines 1-54, `utils/scientific-notation.ts` lines 1-40, `commands/commands/add-decimal.command.ts` lines 1-103, `subtract-decimal.command.ts` lines 1-101, `base/const/currency-symbols.ts` lines 1-138, `base/const/formatdetail.ts` lines 1-121, `base/const/date-format.ts` lines 1-98, `packages/sheets-numfmt-ui/src/menu/number-format-options.ts` lines 1-86; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-app/ui/sheet-numfmt.js` ports the decimal-place edit of a format code (`setPatternDecimal`, `getDecimalString`, `isPatternHasDecimal`), the add and subtract decimal commands (maximum decimals over the selection, the General and scientific-notation cases), the currency format builder (`getCurrencyFormat`) and symbol list, the number and currency negative-style lists, the Korean date and time code list, and the preset menu order and codes. Modifications: quoted text, bracket sections and escapes are masked while a code is edited (the original also splits on a `.` or `;` inside them), a text cell and date, time, fraction and text codes are left alone by the decimal commands, the decimal count reads the first section directly instead of a format-info call, and the preview comes from the sheet engine (`easyoffice-sheet`'s `text_format`). The original's format parser, locale tables, panel components and cell cache are not used.

### Univer sheets cell borders (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets/src/commands/commands/set-border.command.ts` lines 109-176 (`prepareEdgeRange`), 257-319 (inner edges), 353-389 (`outlineBorder`), 389-452 (`clearBorder`); `packages/engine-render/src/components/sheets/sheet.render-skeleton.ts` lines 1764-1800 (`_setBorderProps`); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-app/ui/sheet-look.js` ports the border edit rules: an edge is written on both cells that share it, the ring around a range takes the facing side, and the cell after wins a shared slot. Modifications: an edge is a `"<line>:<rrggbb>"` string on a frozen style object shared through the style intern table, the plan is a list of changed cells with no diagonals, and the grid draws a shared edge once. The reference's border panel, border style manager and canvas renderer are not used.

### Univer sheets row and column insertion and deletion (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets/src/services/ref-range/util.ts` lines 469-517 (`handleBaseRemoveRange`), 630-690 (`handleBaseInsertRange`), 1023-1059 (`runRefRangeMutations`); `packages/sheets-formula/src/controllers/utils/ref-range-formula.ts` lines 361-403 (`handleInsertDelete`); `packages/sheets/src/controllers/merge-cell.controller.ts` lines 529-590 and 723-790; `packages/sheets/src/commands/commands/insert-row-col.command.ts` lines 120-140, `set-row-visible.command.ts`, `set-col-visible.command.ts`, `add-worksheet-merge.command.ts` lines 117-145 and 177-260; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/axis/` (`mod.rs`, `formula.rs`, `layout.rs`), `rules/axis_shift.rs` and `crates/easyoffice-formats/src/xlsx_layout.rs` port, in safe Rust, the span rule for inserted and deleted rows and columns (a span after the edit shifts, one that straddles an insertion grows, one that overlaps a deletion shrinks, one inside it disappears), its use on every formula reference, and the way merged ranges, hidden rows and columns and inserted rows follow an edit. Modifications: the rule works on one axis of a span instead of rotated rectangles, formula text is rewritten by a scanner instead of from tokens, a span that ends on the last row or column keeps ending there, an edit is limited in size, time and the number of formulas it changes and is applied only after every new formula text is known, merging keeps the content of the upper-left cell only, and the hidden and merged parts are stored in XLSX (`hidden`, `mergeCells`). The reference's operator list, mutation and undo objects, selections and React interface are not used.

### Univer and IronCalc sheet view: frozen panes, tab colour, groups (Apache-2.0; MIT OR Apache-2.0)

Sources: https://github.com/dream-num/univer (`packages/core/src/sheets/typedef.ts` lines 450-468 (`IFreeze`), `packages/sheets/src/commands/commands/set-frozen.command.ts` lines 44-58, `set-tab-color.command.ts`, `packages/core/src/types/enum/group-control-toggle-position.ts`; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above) and https://github.com/ironcalc/IronCalc (`xlsx/src/import/worksheets.rs` lines 317-335 (`load_sheet_color`) and 800-905 (the sheet view and `get_split`), `xlsx/src/export/worksheets.rs` lines 660-725 (`pane` with `topLeftCell`, `activePane` and one `selection` per pane); commit 87b684b, used under the MIT license, Copyright (c) IronCalc contributors). `crates/easyoffice-formats/src/sheet_view.rs`, `xlsx_view.rs` and `ods_view.rs` take the model (a frozen pane is a count of rows and a count of columns checked against bounds, the tab colour is one value per sheet, a group's summary sits before or after it) and the XLSX `pane` and `tabColor` shapes, including split counts that are read as numbers because some writers store `6.0`. Modifications: group levels follow ECMA-376 `outlineLevel`, `collapsed` and `outlinePr` because neither project stores groups; a folded group is the set of rows it hid, kept apart from rows a user hid and rows a filter hid; theme tab colours are resolved with the workbook theme and the ECMA-376 HLS tint; ODS uses `tableooo:tab-color`, `table:table-row-group`/`table:table-column-group` with `table:display` and the `ooo:view-settings` split modes; every size has a bound and a value over it is ignored on read and refused on write. `crates/easyoffice-sheet/src/axis/outline.rs` follows the span rule of the row and column insertion entry above for groups and gives inserted rows the smaller level of their neighbours. Selection, zoom and scroll position (PD-229): `crates/easyoffice-formats/src/sheet_view.rs` (`SheetState`, `ViewState`) and `xlsx_view.rs` take from IronCalc (`base/src/types.rs` lines 152-170 (`SheetState` and its file names), `xlsx/src/import/workbook.rs` lines 36-41 (the `state` attribute), `xlsx/src/export/worksheets.rs` lines 640-725 (the `activeCell` and `sqref` of the pane that holds the selection, an empty `selection` for each other pane, the `topLeftCell` of the sheet view and of the pane) and `xlsx/src/import/worksheets.rs` lines 823-895 (one `sheetView`, the `pane` of a frozen sheet); same commit 87b684b, used under the MIT license, Copyright (c) IronCalc contributors). Modifications: the reference reads no selection, zoom or scroll position and writes a selection of one cell; these read the pane the file marks active, keep a range and a zoom of 10 to 400, and drop a value out of its bound instead of clamping it; ODS keeps the cursor cell, the zoom and the first column and row of the panes in the `ooo:view-settings` of `settings.xml`. The unfolded state of a group inside a folded one (`Outline::open_rows`, `open_cols`) has no counterpart in either project and follows ECMA-376 18.3.1.73 (`collapsed` on each summary row) and the `table:display` of each ODF group.

### Univer, IronCalc and Fortune-sheet sheet tabs (Apache-2.0; MIT OR Apache-2.0; MIT)

Sources: https://github.com/dream-num/univer (`packages/sheets-formula/src/controllers/update-formula.controller.ts` lines 575 and 649-691, `packages/sheets-ui/src/views/sheet-bar/sheet-bar-tabs/utils/sheet-tab-name-editor.ts`, `.../utils/sheet-tab-drag-sort.ts` lines 17-90, `packages/sheets/src/commands/commands/copy-worksheet.command.ts`, `remove-sheet.command.ts`, `set-worksheet-hide.command.ts`; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above), https://github.com/ironcalc/IronCalc (`base/src/new_empty.rs` lines 34-37, 296-340, 447-552, `base/src/user_model/common.rs` lines 596-750; commit 87b684bb04a87c63bdd6343c29036450b1cfd358, used under the MIT license, Copyright (c) IronCalc contributors) and https://github.com/ruilisi/fortune-sheet (`packages/core/src/settings.ts` lines 267-276, `packages/react/src/components/ContextMenu/SheetTab.tsx` lines 111-244; commit 94346608877db4747406707a177c4b8f3bacdbf9, MIT License, Copyright (c) 2022 Suzhou Ruilisi Technology Co., Ltd). `crates/easyoffice-sheet/src/axis/sheet_ref.rs` and `rules/sheet_ref.rs` take the decision for references to a renamed sheet (written again with the new name) and a deleted sheet (an error value), `crates/easyoffice-formats/src/xls_sheet_names.rs` the sheet name rules (31 characters, no `\ / ? * : [ ]`), and `crates/easyoffice-app/ui/sheet-tab-names.js`, `sheet-tabs.js`, `sheet-tab-ui.js` and `sheet-tab-menu.js` the copy name `name (N)` cut to fit, the refusal to delete or hide the last shown sheet, the tab menu items, the name typed in the tab (Enter keeps it, Escape leaves it) and the drag target calculation (`dragTarget`). Modifications: references are found by the text scanner of the row and column edit instead of a syntax tree, a deleted sheet's whole range becomes one error value, names are compared without case in both the page and the engine, the copy counter starts at 2, the page shows `Sheet<N>` as `시트 <N>` and counts both spellings as taken, and every change is one undo step made from the workspace snapshot. The references' command, mutation and undo objects, permission checks and React components are not used. Sheet state, copy by a Ctrl drag and 이동/복사 (PD-229): `crates/easyoffice-formats/src/xls_boundsheet.rs` takes from Apache POI (`poi/src/main/java/org/apache/poi/hssf/record/BoundSheetRecord.java` lines 38-39 and 108-150, the position of the `BOF`, the option word whose bit 0 hides and bit 1 hides for good, the name length and encoding flag, then the name; commit 47dcff769bb395034dddfd59c4609659b6dbc6a4, Apache License 2.0, Copyright The Apache Software Foundation; the license text is reproduced in the generated section above) and `sheet_view.rs` (`SheetState`) the `visible`, `hidden`, `veryHidden` states of IronCalc (`base/src/types.rs` lines 152-170, same commit as above). `crates/easyoffice-app/ui/sheet-tab-ui.js` and `shell.css` take from Fortune-sheet (`packages/react/src/components/SheetTab/SheetItem.tsx` lines 99-140 and 209: the drop puts the dragged sheet before the tab it lands on or last, and the tab shows a 2px mark on its left edge while a sheet is over it) and `sheet-view-live.js` its per-sheet zoom restore on a tab switch (`SheetItem.tsx` lines 185 and 199); same commit as above, MIT License. Modifications: the copy by the held Ctrl key (Option on a Mac), the `이동/복사` list and the copy of the names scoped to the source sheet have no counterpart in these projects and follow the spreadsheet program's help pages; the mark uses the accent token of the page and the tabs stay in place while a copy is dragged. Tab colour (PD-229): `crates/easyoffice-app/ui/sheet-tab-strip.js` and `shell.css` take from Univer (`packages/sheets-ui/src/views/sheet-bar/sheet-bar-tabs/SheetBarItem.tsx` lines 38-99: a tab that is not selected has the colour as its background and a light or dark text chosen by how dark the colour is, a selected tab shows the colour as a 2px bar along its lower edge; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd., same entry as above). Modifications: plain DOM and CSS, the text is white on a colour whose brightness `(r*299 + g*587 + b*114)/1000` is under 128 and black otherwise (`packages/core/src/shared/color/color-kit.ts` lines 369-385, same commit), the tab keeps the page's tab shape, and the colour is chosen in the `탭 색` submenu with the style bar's palette.

### Univer, IronCalc and Fortune-sheet defined names (Apache-2.0; MIT OR Apache-2.0; MIT)

Sources: https://github.com/dream-num/univer (`packages/engine-formula/src/services/defined-names.service.ts` (the fields of a name, the lookup that prefers a sheet's own name over the workbook's, one lower-cased index), `packages/sheets/src/basics/defined-name-utils.ts` (`validateDefinedName`: empty, duplicate, invalid, the name of a sheet or of a function), `packages/sheets-ui/src/views/defined-name/DefinedName.tsx`, `DefinedNameOverlay.tsx` and `packages/sheets-ui/src/services/utils/defined-name-utils.ts` (the name box: a typed name selects its range, an address selects that range, a new valid name names the selection, the list with its manager entry), `packages/sheets/src/commands/commands/set-defined-name.command.ts` and `insert-defined-name.command.ts` (a change is one undoable step); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above), https://github.com/ironcalc/IronCalc (`base/src/model.rs` (`DefinedNameEntry`, `is_valid_defined_name`, the comment of at most 255 characters, a name for a reference, a range or any formula, a definition that does not parse reads as an error), `xlsx/tests/hidden_defined_names.rs` (the `hidden` and `comment` attributes survive a round trip); commit 87b684bb04a87c63bdd6343c29036450b1cfd358, used under the MIT license, Copyright (c) IronCalc contributors) and https://github.com/ruilisi/fortune-sheet (`packages/react/src/components/FxEditor/NameBox.tsx`: the project's name box only shows the selection's address and has no names; commit 94346608877db4747406707a177c4b8f3bacdbf9, MIT License, Copyright (c) 2022 Suzhou Ruilisi Technology Co., Ltd). `crates/easyoffice-formats/src/sheet_names.rs` and `crates/easyoffice-sheet/src/names.rs` take the model (name, scope, formula, comment, hidden), the duplicate rule (the same name, compared without case, once per scope), the lookup order and the checks of a new name; `crates/easyoffice-app/ui/sheet-names.js` and `sheet-names-ui.js` take the name box flow and its list. The behaviour of the LibreOffice spreadsheet for `table:named-range` and `table:named-expression` (MPL-2.0; no code taken) guided `crates/easyoffice-formats/src/ods_names.rs`. Modifications: a name is resolved when a formula is compiled (the definitions are expanded once in dependency order and a formula gets a copy of the tree, so the dependency graph, the evaluator and the functions that look at the shape of their arguments see plain references), a cycle, a chain deeper than 64 names and an expansion past 4,096 nodes (8,192 for a formula) are found without recursion and read as errors, a name is moved with the cells by the same text scanner that moves a formula (`crates/easyoffice-sheet/src/axis/names.rs`), and a definition is a formula text with the syntax of ECMA-376 18.2.5 (a letter, `_` or `\` first; never a cell or row/column address). The references' services, observables, commands, mutations and React components are not used.

### Univer sheets sort (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets-sort/src/controllers/utils.ts` lines 1-125 (`compareNull`, `compareNumber`, `compareString`), `controllers/sheets-sort.controller.ts` lines 1-92 (the common comparison), `commands/commands/sheets-sort.command.ts` lines 1-161 (`reorderFnGenerator`, which rows are collected), `services/sheets-sort.service.ts` lines 1-140 (`mergeCheck`, `isRangeDividedEqually`); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/sort_keys.rs` ports the comparison (blank cells after every other value in both directions, numbers before text, text compared in lower case without `-` and `'`, the first key that differs decides), `crates/easyoffice-app/ui/sheet-sort.js` ports the rows that move (rows hidden by the user or by a filter stay in place, merged cells accepted only when equal merges tile the range, rows then move as blocks of the merge height) and the header-row option. Modifications: the engine sorts stably by construction and compares prepared keys held in one arena instead of lower-casing both texts on every comparison, a case-sensitive option puts a lowercase letter before the same uppercase letter, a key inside a merge reads the merge's own cell, the number of keys is limited to 64, and formulas of moved rows are rewritten by the engine (`sort_refs.rs`). The reference's command and undo objects, selection handling and dialog are not used.

### Univer sheets data validation kinds and time period rules (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/core/src/types/enum/data-validation-type.ts` lines 1-38, `packages/sheets-data-validation/src/validators/checkbox-validator.ts` lines 1-169, `list-multiple-validator.ts` lines 1-28, `list-validator.ts` lines 150-200 (`isValidType`), `validators/util.ts` lines 1-87; `packages/sheets-conditional-formatting/src/base/const.ts` lines 32-43 (`CFTimePeriodOperator`) and `models/calculate-unit-v2/highlight-cell-calculate-unit.ts` lines 149-240 and 314-322 (the `timePeriod` rule); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/rules/validate.rs` and `crates/easyoffice-app/ui/sheet-dv-widgets.js` port the check box rule (a cell holds the checked or the unchecked value, equal values are refused) and the multiple list rule (every comma separated item must be an item of the list), `crates/easyoffice-sheet/src/rules/period.rs` and `cf.rs` port the ten periods and the match of a numeric cell against the period's days. Modifications: the check box values default to `TRUE` and `FALSE` (the reference uses 1 and 0), a value comparison ignores case, days are spreadsheet serial numbers and weeks run Sunday to Saturday, the day "today" is passed in by the page (its local date), and both new validation kinds are stored in XLSX as a list plus a private extension that names them. The reference's formula service, widget rendering and dialog are not used.

### statrs (MIT)

Source: https://github.com/statrs-dev/statrs (`src/function/gamma.rs`, `src/function/beta.rs`). `crates/easyoffice-sheet/src/special.rs` ports its Lanczos `ln_gamma` (g = 10.900511), the regularized incomplete gamma series and continued fraction, and the regularized incomplete beta continued fraction in safe Rust with bounded loops; the distribution inverter is written in this repository. Used under the MIT license:

```
MIT License

Copyright (c) 2016 Michael Ma

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

### PDF text markup annotations (appearance reference)

`crates/easyoffice-formats/src/pdf_highlight.rs` writes highlight and underline annotations (ISO 32000-1 §12.5.6.10) through the `pdf-writer` crate (PD-269 step 4; its licence text is the pdf-writer entry of the generated section above). The highlight appearance follows the pdf.js editor (https://github.com/mozilla/pdf.js, Apache-2.0): a rectangle filled with the color under the multiply blend mode. No source of either project is copied; the notice is kept because the appearance is derived from pdf.js.

### quick-xml (MIT)

Source: https://github.com/tafia/quick-xml (version 0.42.0, no optional feature). Linked by `crates/easyoffice-formats/src/xml_stream.rs`, which turns the crate's events into the start, end and text events the document readers consume. No code of the crate is copied (before PD-269 step 3 the file was a port of its pull reader). The refusal of `DOCTYPE`, the event and nesting budgets, the lenient entity rules and the attribute checks around it are the engine's own. Used under the MIT license, whose text is the quick-xml entry of the generated section above (Copyright (c) 2016 Johann Tuffe).

### calamine (MIT)

Source: https://github.com/tafia/calamine 0.38.0 (`src/datatype.rs` `Data` variants, `src/xlsx/mod.rs`: `read_shared_strings`, `ST_CellType` dispatch and the worksheet cell reader). `crates/easyoffice-formats/src/xlsx_sheet.rs`, `ods_grid.rs` (from `src/ods.rs`) and `cell_type.rs` (PD-241, 1-byte `CellValueKind` and sparse `FastU64Map` overrides) follow its single event loop with a small state per cell, explicit value-kind dispatch and a shared string table built once, in safe Rust without the dependency; the table is an arena string with end offsets. The cell rules (formulas, shared formulas, inline strings, types) are this repository's own. Used under the MIT license (Copyright (c) 2016 Johann Tuffe), whose text is the quick-xml entry of the generated section above.

### Univer sheets auto filter (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets-filter/src/models/types.ts` lines 22-136, `models/filter-model.ts` lines 31-580, `models/custom-filters.ts` lines 52-192, `controllers/sheets-filter.controller.ts` lines 268-518, `commands/commands/sheets-filter.command.ts` lines 63-330, `packages/sheets-filter-ui/src/models/conditions.ts` lines 79-727, `src/worker/generate-filter-values.service.ts` lines 93-160 and 349-362; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-formats/src/sheet_filter.rs` and `xlsx_filter.rs` port the filter data shapes (range, per-column list of values with the blank switch, one or two custom conditions joined by AND or OR, the operator set) and map them to the OOXML `autoFilter` element. `crates/easyoffice-sheet/src/filter/` (`condition.rs`, `mod.rs`) ports the column conditions (numeric operators accept number cells only, "not equal" accepts every other cell, a text condition is a whole-text wildcard match, a pattern of one space is "not empty"), the evaluation over the rows below the header with rows hidden by another column skipped, and the value list of a column over the rows the other columns leave visible (distinct texts, counts, empty cells counted apart, numbers first). `crates/easyoffice-sheet/src/axis/filter.rs` ports how the range and the column conditions follow row and column insertion and deletion. `crates/easyoffice-app/ui/sheet-filter.js` ports the toggle rule for the filter range (a single cell takes the region around it, a single row the rows below it), the re-apply on request without an undo step, and the mapping between the condition dialog and the custom conditions (wildcards for contains, starts with and ends with, a single space for "not empty"). Modifications: the matcher is a compiled bit-parallel wildcard search that runs in time linear in the text (the reference builds a regular expression), letter case is ignored for "not equal" patterns, a plain number written as text equals the number cell with that value, the hidden rows are a bit per row and kept apart from the user's hidden rows, conditions are judged on the displayed text of the cell, every list and text has a bound, and the colour filters, date grouping tree, worker and render plumbing of the reference are not used.

### Univer sheets formula functions (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/engine-formula/src/functions/lookup/{lookup,address,areas,formulatext,hyperlink}/index.ts`, `functions/information/isformula/index.ts`, `functions/database/{daverage,dcount,dcounta,dget,dmax,dmin,dproduct,dstdev,dstdevp,dsum,dvar,dvarp}/index.ts`, `functions/math/{mmult,mdeterm,minverse,munit}/index.ts`, `basics/database.ts`, `basics/math.ts` lines 74-250; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/functions_lookup_ref.rs`, `functions_db.rs` and `functions_matrix.rs` port, function by function and in safe Rust, the argument and error rules of the six lookup and reference functions, the database, field and criteria checks with the row-wise AND / column-wise OR criteria rule and the aggregation of the twelve database functions, and the LU decomposition, determinant and inverse of the matrix functions. Modifications: criteria are compiled once into the shared `COUNTIF` matcher and records are read one at a time, matrices are flat row-major arrays with an `i-k-j` multiplication, every size is bounded and an oversized input is `#VALUE!`, a relative R1C1 `ADDRESS` is counted from the formula cell, and `HYPERLINK` accepts only `http`, `https` and `mailto` targets and `#` jumps inside the workbook. The reference's value objects, array results and function registry are not used.

### Univer sheets formula functions, bundle B (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/engine-formula/src/functions/financial/{accrint,accrintm,amorlinc,coupdaybs,coupdays,coupdaysnc,coupncd,coupnum,couppcd,disc,duration,ispmt,mduration,oddfprice,oddfyield,oddlprice,oddlyield,price,pricedisc,pricemat,received,tbilleq,tbillprice,tbillyield,vdb,yield,yielddisc,yieldmat}/index.ts`, `functions/engineering/{besseli,besselj,besselk,bessely,convert,imcosh,imcot,imcoth,imcsc,imcsch,imlog,imsec,imsech,imsinh,imtan,imtanh}/index.ts`, `functions/statistical/{frequency,linest,logest,trend,growth,prob,mode-mult,maxa,mina,stdeva,stdevpa,vara,varpa}/index.ts`, `functions/information/isnontext/index.ts`, `functions/web/encodeurl/index.ts`, `functions/base-function.ts` (`flattenArray`), `basics/financial.ts`, `basics/date.ts` (`getTwoDateDaysByBasis` and the day-count helpers), `basics/engineering.ts` lines 17-340 (the `BESSEL` namespace), `basics/complex.ts` (the hyperbolic, reciprocal and logarithm methods), `basics/statistical.ts` lines 812-1177, `basics/math.ts` lines 120-262, `engine/utils/check-variant-error.ts`; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/functions_coupon.rs`, `functions_bond.rs`, `functions_odd.rs`, `functions_depr.rs`, `functions_fin_args.rs`, `functions_bessel.rs`, `functions_imtrig.rs`, `functions_convert.rs`, `functions_trend.rs`, `functions_regression.rs` and `functions_stat_a.rs` port, function by function and in safe Rust, the argument and error rules of the 59 functions, the coupon schedule, the day-count conventions, the bond price and the secant and bracketing yield solvers, the odd-period price formulas, the Bessel approximations and recurrences, the unit table with its prefixes, the complex hyperbolic and reciprocal formulas, the regression with its statistics rows and the Householder least-squares solve. The unit tests of the reference are reproduced as data-driven cases in `crates/easyoffice-sheet/tests/sheet_univer_functions_b_*.rs`. Modifications: dates are whole serial numbers in the 1900 date system only, every coupon walk, solver, Bessel order, depreciation walk and regression is bounded and over the bound the result is an error, bond prices are computed from terms found once per call, the unit symbols are found through a hash map and rows whose symbol the reference stores garbled are left out, the rank-deficient regression fallback is a Jacobi pseudoinverse of the normal matrix, a blank cell in a range is skipped by the `*A` statistics, `LOGEST` and `GROWTH` reject a known y that is not positive, `FREQUENCY` counts with sorted edges and a binary search, and a matrix result is the top-left cell in a cell and the whole matrix under `INDEX` and the aggregates. The reference's value objects, element-wise array broadcasting and function registry are not used.

### Univer sheets formula functions, bundle C (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/engine-formula/src/functions/math/{acosh,asinh,atanh,acot,acoth,cot,coth,csc,csch,sec,sech,ceiling-math,ceiling-precise,floor-math,floor-precise,roundbank,aggregate}/index.ts`, `functions/text/{lenb,leftb,midb,rightb,findb,searchb,replaceb,asc,dbcs}/index.ts`, `functions/statistical/{f-dist,f-dist-rt,f-inv,f-inv-rt,expon-dist,weibull-dist,lognorm-dist,lognorm-inv,poisson-dist,negbinom-dist,hypgeom-dist,binom-dist-range,confidence-norm,confidence-t,chisq-test,f-test,t-test,average-weighted,marginoferror}/index.ts`, `functions/compatibility/{function-map.ts,hypgeomdist,lognormdist,negbinomdist}`, `engine/utils/char-kit.ts`, `engine/utils/math-kit.ts` lines 44-76, `basics/statistical.ts` lines 745-809 and 1170-1724; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/functions_math_trig.rs`, `functions_aggregate.rs`, `functions_text_bytes.rs`, `byte_width.rs`, `functions_dist_more.rs` and `functions_stat_tests.rs` port, function by function and in safe Rust, the argument and error rules of the reciprocal and inverse hyperbolic functions, the significance rounding functions with the epsilon-corrected `ceil` and `floor`, bankers rounding, the 19 function codes and 8 options of `AGGREGATE`, the byte text functions, the domains of the distribution functions, the chi-square, F and t tests, the weighted average and the margin of error, and the legacy-name table. Modifications: byte widths follow a fixed table (Hangul, Han, kana and the full-width forms are 2 bytes) and a byte boundary inside a 2-byte character gives a space, `ASC` and `DBCS` also convert the katakana and the full-width symbols, the distribution values come from the regularized incomplete beta and gamma functions with the tails computed directly, the hypergeometric sum starts at the mode, hidden rows are not known to the formula engine, only numbers count in a range, and sums and searches are bounded. The reference's value objects, array broadcasting, formula service and number-format handling are not used.

### Univer sheets auto fill and paste special (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/sheets/src/services/auto-fill/tools.ts`, `rules.ts`, `packages/sheets/src/controllers/auto-fill.controller.ts`, `packages/sheets-ui/src/controllers/auto-fill-ui.controller.ts`, `packages/sheets-ui/src/services/clipboard/clipboard.service.ts` lines 135-140 and 894-896, `packages/sheets-ui/src/controllers/clipboard/clipboard.controller.ts` lines 665-862; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-app/ui/sheet-fill-core.js` ports the equal-step and equal-ratio tests, the least squares forecast, the number series, the text-with-number counter, the cyclic list fill, the grouping of neighbouring cells of one kind, the copy-first rule for single numbers and the detection of the double click target. `sheet-paste-core.js` follows the meaning of the paste modes (value, format, column width). Modifications: the cycle lists are Korean and English, month names are tried before the counter rule, the cycle stride is the list length, a negative position wraps by modulo, the first list that holds a text wins, the regression is fitted once per run, a fill above or to the left repeats the source from its far end, a fill is refused above 100,000 lines or 1,000,000 cells, and the turn-over of rows and columns follows the behaviour of another office engine for the references of a copied formula (the reference leaves it unwritten; behaviour only, no code was taken). The reference's command, undo and rendering layers are not used.

### Univer, Fortune-sheet and an XLSX library's test data, sheet and workbook protection (Apache-2.0; MIT; Apache-2.0)

Sources: https://github.com/dream-num/univer (`packages/sheets/src/services/permission/permission-point/worksheet/*.ts` (one permission point per action: cell value, cell style, row and column style, insert and delete of rows and columns, sort, filter, select protected and unprotected cells, hyperlink, extra objects, pivot table), `packages/sheets/src/commands/commands/set-worksheet-protection.command.ts` and `add-range-protection.command.ts` (a change of a protection is one undoable step), `packages/sheets/src/models/range-protection-rule.model.ts`; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above), https://github.com/ruilisi/fortune-sheet (`packages/core/src/modules/protection.ts`: a sheet's `authority` holds the allowed actions, a cell is locked unless it carries its own flag, selecting locked and unlocked cells are two actions; commit 94346608877db4747406707a177c4b8f3bacdbf9, MIT License, Copyright (c) 2022 Suzhou Ruilisi Technology Co., Ltd) and the open-source Java spreadsheet library whose test data (`test-data/spreadsheet/workbookProtection-sheet_password-2013.xlsx`, `workbookProtection-workbook_password-2013.xlsx`, `workbookProtection-workbook_password_user_range-2010.xlsx`, Apache License 2.0) holds files a spreadsheet program wrote: the `algorithmName`, `hashValue`, `saltValue` and `spinCount` of three of them and the legacy `CBEB` are copied as attribute text into `crates/easyoffice-formats/tests/sheet_protect_roundtrip.rs`, and its description of the agile loop (the previous digest, then the counter, for a sheet; 100,000 rounds) was compared with `crates/easyoffice-formats/src/sheet_protect.rs`. `crates/easyoffice-formats/src/sheet_protect.rs` takes the set of actions and the rule that a cell is editable when its own flag, its row or column, or an allowed range says so; `crates/easyoffice-app/ui/sheet-protect.js` the guards. Modifications: the flags are the attributes of ECMA-376 18.3.1.85 (a set flag locks the action), the digests are SHA-1, SHA-256, SHA-384 and SHA-512 written in `crates/easyoffice-formats/src/sha_digest.rs` from FIPS 180-4, a `spinCount` over 10,000,000, an unknown algorithm or text that is not base64 is kept and never opens, and every edit entry point asks the guard. The behaviour of the LibreOffice spreadsheet for `table:protected`, `table:protection-key` and `loext:table-protection` (MPL-2.0; no code taken) guided `crates/easyoffice-formats/src/ods_protect.rs`. The references' commands, permission services and components are not used.

### IronCalc dynamic array functions (MIT OR Apache-2.0)

Source: https://github.com/ironcalc/IronCalc (`base/src/functions/spill_functions.rs` lines 19-44, 60-117, 192-656 (`fn_sort`, `fn_sortby`, `fn_unique`, `fn_filter`, the sort key order and the equality key), `base/src/functions/logical/let.rs` lines 111-160, `lambda.rs` lines 13-62 and 135-180, `map_reduce.rs` lines 34-147, `scan.rs` lines 33-104, `bycol_byrow.rs` (`fn_bycol`, `fn_byrow`, `fn_makearray`), `base/src/functions/math_and_trigonometry/sequence.rs` lines 10-125, `random.rs` lines 58-190 (`fn_randarray`), `array_size.rs`; the tests `base/src/test/test_spill_functions.rs`, `test_fn_lambda.rs`, `test_fn_let.rs`, `test_fn_sequence.rs`, `test_fn_randarray.rs` and the generator of `base/src/test/dynamic_evaluation/test_order_independence.rs`; commit 6dc9e8bae4b8d066276f399de7b3e1cf991c220b, used under the MIT license or the Apache License 2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH). `crates/easyoffice-sheet/src/functions_spill.rs`, `functions_lambda.rs`, `functions_array_gen.rs` and `tests/sheet_dynamic_*.rs` port the function bodies and the cases onto this engine's grid and error types. Modifications: the reference patches variable ids into a copy of the syntax tree for each call, this engine binds variables in a scope that closures capture; the reference keeps an order of evaluation with restarts, this engine publishes the cells an array fills after a pass and recalculates what the publishing touched; `FILTER` requires the include array to be as long as the rows or columns, an array that cannot be placed or is over the budget is `#SPILL!`, a lambda that returns an array is `#CALC!`, a call with the wrong number of arguments is `#VALUE!`, and the cell budget is the evaluation step budget. Optional `LAMBDA` parameters and the `TAKE`, `DROP`, `TOCOL`, `TOROW`, `VSTACK`, `HSTACK`, `CHOOSEROWS`, `WRAPROWS` family are not taken.

### IronCalc array shaping functions (MIT OR Apache-2.0)

Source: https://github.com/ironcalc/IronCalc (`base/src/functions/lookup_and_reference/drop_take.rs` (`fn_take`, `fn_drop`), `tocol_torow.rs` (`fn_tocol`, `fn_torow`, `flatten_array_impl`), `hstack_vstack.rs`, `choosecols_chooserows.rs`, `expand.rs`, `wrapcols_wraprows.rs`, `information/isomitted.rs`; the tests `base/src/test/test_fn_drop_take.rs`, `test_fn_tocol_torow.rs`, `test_fn_isomitted.rs`; commit 01b39c1ad26252f7b6c810440e8433a932daee07, used under the MIT license or the Apache License 2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH). `crates/easyoffice-sheet/src/functions_array_shape.rs` and `tests/sheet_dynamic_take_drop.rs`, `sheet_dynamic_tocol_torow.rs`, `sheet_dynamic_isomitted.rs` port the function bodies and the cases onto this engine's grid. Modifications: a size left out (`TAKE(a,,2)`) means the whole side, every result is checked against the array budget, `#N/A` pads a short side, a call with the wrong number of arguments is `#VALUE!`, and an omitted `LAMBDA` parameter is a variable of the scope bound to nothing (`Val::Omitted`) instead of an empty argument.

### IronCalc text, regular expression, lookup, information, math and financial functions (MIT OR Apache-2.0)

Source: https://github.com/ironcalc/IronCalc (`base/src/functions/text/textsplit.rs` lines 25-281 (`fn_textsplit`, `extract_delimiters`, `split_by_any`), `text/arraytotext.rs` lines 13-113 (`fn_arraytotext`, `format_array_value`), `text/regex_functions.rs` lines 24-148 (`fn_regexextract`, `fn_regexreplace`, `fn_regextest`), `lookup_and_reference/trimrange.rs` lines 8-134 (`is_blank`, `row_is_empty`, `col_is_empty`, `fn_trimrange`), `information/mod.rs` lines 108-114, 259-338, 376-389, 394-481 and 485-526 (`fn_isref`, `fn_sheet`, `fn_sheets`, `fn_cell`, `fn_info`), `math_and_trigonometry/mathematical.rs` lines 852-855 and 1265-1281 (`fn_iso_ceiling`, `fn_percentof`), `financial/financial_depreciation.rs` lines 147-333 and 494-574 (`amor_days_in_year`, `amor_fix_29_february`, `amor_first_depr_linc`, `amor_depr_coeff`, `amor_are_equal`, `amor_round`, `amor_degrc`, `fn_amordegrc`); the tests of `base/src/test/**` listed in the next entry; commit f93662006b8fb1c401dc870bbda26c5d60ac6037, used under the MIT license or the Apache License 2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH). `crates/easyoffice-sheet/src/functions_text_split.rs`, `functions_regex.rs`, `functions_info_ref.rs`, `functions_array_shape.rs` (`fn_trimrange`), `functions_math_trig.rs` (`eval_percentof`, `ISO.CEILING`) and `functions_depr.rs` (`fin_amordegrc`) port the function bodies, with the reference's function and helper names, branch order and error values, onto this engine's values, errors and array evaluator through thin adapters. Modifications: `TEXTSPLIT` cuts on `char` positions with a one-to-one case fold (the reference lower-cases the whole text and reuses byte offsets), bounds the work (`text length * delimiters`) and the result size, and limits a result text to the cell text size; `ISREF` treats a reference as what this engine's reference resolver gives (`OFFSET`, `INDIRECT`, `INDEX`, `CHOOSE`, names) and an array an earlier step made is no reference; `CELL("filename")` and the info types that need a file name, a directory or a release number are `#N/A`; `SHEETS` with a reference counts one sheet; `AMORDEGRC` takes its day counts from this engine's basis-aware day count functions. The regular expression functions compile the pattern with a length limit and the `regex` crate's size, DFA cache and nesting limits (the crate is used under the MIT license or the Apache License 2.0, `regex` 1.x as locked), read the `return_mode`, case sensitivity and `occurrence` arguments the functions document, and `return_mode` 0 gives the whole first match where the reference gives the first group.

### IronCalc function test cases (MIT OR Apache-2.0)

Source: https://github.com/ironcalc/IronCalc (`base/src/test/` files that only set cells, calculate and read cell text, commit f93662006b8fb1c401dc870bbda26c5d60ac6037, used under the MIT license or the Apache License 2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH). `crates/easyoffice-sheet/tests/sheet_ironcalc_cases/` holds, one module per reference file, the 533 cases this engine gives the same result for (the module name is the reference path with `/` as `_`; 82 files). Modifications: the setup helper is this engine's `dynamic_support` model, product names in comments are replaced by roles, and the cases and assertions whose result differs (internal error codes, 15-digit rounding correction) are left out (counted in `docs/sheet-function-parity.md`).

### IronCalc rounding functions (MIT OR Apache-2.0)

Source: https://github.com/ironcalc/IronCalc (`base/src/functions/math_and_trigonometry/mathematical.rs` lines 590-660 (`fn_round`, `fn_roundup`, `fn_rounddown`), 702-767 (`fn_floor`, `fn_ceiling`), 930-978 (`fn_mround`), 980-1000 (`fn_trunc`), 1310-1362 (`Rounding`, `round_to_digits`, `round_result`) and `base/src/number_format.rs` lines 117-127 and 220-226 (`to_precision`, `to_excel_precision`), commit f93662006b8fb1c401dc870bbda26c5d60ac6037, used under the MIT license or the Apache License 2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH). `crates/easyoffice-sheet/src/functions_math.rs` and `crates/easyoffice-sheet/src/sig_digits.rs` hold the functions that take the number of ROUND, ROUNDUP, ROUNDDOWN and TRUNC, and the ratio of CEILING and FLOOR, at 15 significant digits before cutting. Modifications: the argument reading and error values are this engine's, `to_precision` and `to_excel_precision` are one function (the `ryu` print and parse of the original returns the same value), and the original MROUND structure (no correction) replaces this engine's earlier ulp nudge.

### Univer and IronCalc sheet undo history (Apache-2.0; MIT OR Apache-2.0)

Sources: https://github.com/dream-num/univer (`packages/sheets/src/commands/mutations/set-range-values.mutation.ts` lines 114-154 (`SetRangeValuesUndoMutationFactory`: the undo of a change is the same change carrying the old value of every cell it names), `packages/core/src/services/undoredo/undoredo.service.ts` lines 112 (`DEFAULT_UNDO_REDO_HISTORY_LIMIT`), 219-263 (`pushUndoRedo`: a new item empties the redo stack, the oldest leaves past the limit) and 313-336 (`popUndoToRedo`, `popRedoToUndo`); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above) and https://github.com/ironcalc/IronCalc (`base/src/user_model/history.rs` lines 11-23 (`RowData`, `ColumnData`) and 26-331 (`Diff`: one entry per kind of change holding the old value, the row and column variants keeping the cells they delete, `SetMergedCells` keeping the whole old list) and 333-366 (`DiffList`, `History`, a list of diffs per user action on an undo and a redo stack); commit d50f26e0bc618271c0f3b398238b04ad6ba12c05, used under the MIT license, Copyright (c) IronCalc contributors). `crates/easyoffice-app/ui/sheet-delta.js` takes the first engine's inverse-mutation pattern (the record of a change is the old value of the cells it names, applying a record returns the record that leads back) and stores the cells as a struct of arrays (packed addresses, flag bytes, shared strings); `sheet-history.js` takes the second engine's list of changes per user action with old values, one list for the cells of each sheet and one for the settings that changed, and its way of keeping the cells a row or column delete removes. Modifications: the record is built from the cells the change names (or, for changes whose cells are not known before, from a read of every cell compared with the result afterwards), cells the change left as they were are dropped, a row or column edit is stored as the edit that leads back plus the cells it changed or deleted (the cells that only moved are not stored), the redo is built when the undo runs so it holds what the undo found, the stacks are bounded in steps (100) and in bytes (192 MiB) where the first engine has a step limit only and the second none, and a record over the byte bound ends the history. Fortune-sheet (MIT) keeps patches and inverse patches of an immutable state tree, which needs the whole workbook to be such a tree and was not taken.

### Univer BAHTTEXT and EUROCONVERT (Apache-2.0)

Source: https://github.com/dream-num/univer (`packages/engine-formula/src/functions/text/bahttext/index.ts` lines 24-122 (`calculate`, `_handleSingleObject`, `_convertNumberToThaiText`) and `text/euroconvert/index.ts` lines 24-184 (`EURO_CURRENCIES`, `calculate`, `_getCurrency`, `_getTriangulationDecimals`) with the tests `bahttext/__tests__/index.spec.ts` and `euroconvert/__tests__/index.spec.ts`; commit 4767b3b309467c283bbd52b725536ca73861e0fc, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/functions_text_locale.rs` ports the Thai number reading (the digit, place and special word tables and the place loop), the satang and baht branches, the fixed euro rate table with its calculation precisions, the triangulation through the euro with the rounded intermediate value, the `CInt` reading of the triangulation precision and the order of the argument checks, through thin adapters onto this engine's values and errors. Modifications: the satang count is taken from the value rounded to two decimals and a count of 100 is carried into the baht (the reference multiplies a binary fraction by 100 and reads the digits of the product), a value of 10^15 or more is `#VALUE!`, rounding is this engine's 15-digit rounding, and the tests are written as formulas on this engine's test model. `JIS` runs the conversion of the `DBCS` port described in an earlier entry (the two names are one function). `PHONETIC` has no reference.

### Univer and IronCalc sheet hyperlinks (Apache-2.0; MIT OR Apache-2.0)

Sources: https://github.com/dream-num/univer (`packages/sheets-hyper-link/src/types/interfaces/i-hyper-link.ts` lines 17-47 (a link: address, tooltip, cell), `models/hyper-link.model.ts` (one link per cell), `services/parser.service.ts` lines 50-91 (an address that starts with `#` is a jump inside the workbook), `controllers/ref-range.controller.ts` lines 154-216 (a link to a range moves, shrinks or turns to an error with the range), `packages/engine-formula/src/functions/lookup/hyperlink/index.ts` (element-wise array lifting of `HYPERLINK` and attachment of the target onto the returned value object so pass-through wrappers preserve it while operators strip it, PD-236) and `packages/sheets-hyper-link-ui/src/controllers/auto-fill.controller.ts` lines 103-136 and 145-153, `copy-paste.controller.ts` lines 157-166 (the copy and series fills and the plain paste bring the link, the special pastes leave links alone); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above) and https://github.com/ironcalc/IronCalc (`base/src/types.rs` lines 172-190 (`Link`: an address outside the workbook or a location inside it, the displayed text is the cell's own content), `xlsx/src/export/worksheets.rs` lines 71-153 (`get_sorted_links`, `split_external_target`, `get_hyperlinks_section`, `get_worksheet_xml_rels`) and `xlsx/src/import/worksheets.rs` lines 686-806 (`load_hyperlinks`, the hyperlink relationships of a sheet, `MAX_HYPERLINK_RANGE_CELLS`); commit 01b39c1ad26252f7b6c810440e8433a932daee07, used under the MIT license or the Apache License 2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH). `crates/easyoffice-formats/src/sheet_links.rs`, `xlsx_links.rs`, `ods_links.rs`, `crates/easyoffice-sheet/src/axis/links.rs`, `crates/easyoffice-sheet/src/functions_lookup_ref.rs`, `array.rs`, `array_eval.rs`, `spill.rs` and `crates/easyoffice-app/ui/sheet-link.js`, `sheet-link-ui.js`, `sheet-spill.js`, `sheet-paste-core.js`, `sheet-fill-core.js`, `sheet-cell-write.js` port the model, the move rules, the formula hyperlink propagation and the file element layout onto this engine's cell and format types. Modifications: the first reference removes the link when the cell is written or cleared, here the link stays through any text the cell is given and goes with a cut, a paste or fill of cells or the removal command; the first reference keeps links apart from the cells and moves them with a reference-range service and names the target sheet by id, here the link is a field of the cell entry, an in-workbook target names its sheet and the engine rewrites the target text with the formula reference scanner; instead of heap-allocating link strings on every value object during recalculation, normal evaluation tracks a non-zero `u32` handle lazily on `Grid` (0 heap bytes for non-link formulas) and materializes the URL string pool only when a cell or spilled child is clicked (`PD-236`), and bare email AutoCorrect uses a 128-byte ASCII lookup table in a single pass; the second reference stops expanding a link range at 10,000 cells (the first cell only), here the same, plus bounds on the links of a sheet and of a document, a policy on every address, an ODF `text:a` reader and writer, and the count of links turned away. Formatted-text links inside a cell are not taken.

### IronCalc and Univer arrays next to merged ranges and run-time references (MIT OR Apache-2.0; Apache-2.0)

Sources: https://github.com/ironcalc/IronCalc (`base/src/model.rs` lines 1378-1445 (`spill_dynamic_array`: a cell of the area that lies in a merged range blocks the array, another array's cells block it, the pass is not applied when it contradicts a read), `base/src/evaluation.rs` lines 134-170 (`Seen`, `SeenRecord`), 484-500 and 1044-1060 (`record_seen`), 1123-1150 (`spill_contradicts_a_read`); commit 01b39c1ad26252f7b6c810440e8433a932daee07, used under the MIT license, Copyright (c) IronCalc contributors) and https://github.com/dream-num/univer (`packages/engine-formula/src/engine/dependency/formula-dependency.ts` lines 1136-1160 (`_buildDirtyRangesByAddressFunction`), 1191-1262 (`_calculateAddressFunction`), 1320-1360 (`_addDependencyTreeByAddressFunction`, `_getRangeListByFunctionRefNode`: the reference-returning functions are run to learn their ranges and the ranges become dependencies of the formula) and `packages/engine-formula/src/services/runtime.service.ts` lines 1055-1160 (`_checkIfArrayFormulaRangeHasData`, `_isInOtherArrayFormulaRange`: what blocks an array, read for comparison only); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-sheet/src/spill_merges.rs` takes the rule that a merged range inside the area blocks the array (`#SPILL!`), `crates/easyoffice-sheet/src/dyn_reads.rs` and `recalc/dynamic.rs` take the way a run-time reference becomes a dependency (the range it resolved to is written down and reads of the next round find it) together with the recorded read of IronCalc. Modifications: the merges are kept sorted by first row with the tallest height so one binary search answers "does a merge touch this rectangle" for the whole area, the merge of the anchor's own cell counts (the reference skips the anchor), the ranges are the ones the real calculation resolved (not a separate analysis step) and are kept in a second dependency graph that shares the interval index of the first, a formula past 32 ranges or a graph past 262,144 edges is calculated again whenever a round filled cells, and a pass is not restarted: the cells an array filled are published after the pass and the readers recorded for them are calculated in the next round (up to 64 rounds, then the circular-reference error). The references' worksheet model, restart driver, worker-side formula tree and ordering of address functions are not used.

### Apache POI BIFF8 hyperlinks (Apache-2.0)

Source: https://github.com/apache/poi (`poi/src/main/java/org/apache/poi/hssf/record/HyperlinkRecord.java` lines 55-64 (the flag bits and the 24-byte tail of a URL moniker), 309-405 (reading an `HLink` record) and 410-455 (writing it); commit 47dcff769bb395034dddfd59c4609659b6dbc6a4, Apache License 2.0, Copyright The Apache Software Foundation; the full license text is reproduced in the generated section above and the project's NOTICE is reproduced under "Legacy binary templates (Apache POI, Apache-2.0)"). `crates/easyoffice-formats/src/xls_links.rs` ports the record layout (range, class id, stream version, flags, display name, frame name, URL moniker with its length and tail, location; a string counts its terminating zero) in safe Rust. Modifications: only a URL moniker, a moniker saved as a string and a location are read (a file moniker or a path is turned away and counted), every length is checked against the bytes of the record, every address goes through the address policy of the sheet links, the ranges are charged to the document's allowance of cells to visit, no display name is written, and the screen tip record `HLinkTooltip` (MS-XLS 2.4.141) is read and written, which the reference does not support. The reference's record classes, stream classes and cell range type are not used.

### Univer and Fortune-sheet auto filter colour, date groups and table filters (Apache-2.0; MIT)

Sources: https://github.com/dream-num/univer (`packages/sheets-filter/src/models/types.ts` lines 71-79 (`IColorFilters`), `models/filter-model.ts` lines 395-460 and 496-524 (`FilterColumn.calc` for colour columns, `filterByColorsFnFactory`: a cell stands for its fill or its text colour, a cell with none is its own value), `packages/sheets-filter-ui/src/worker/generate-filter-values.service.ts` lines 206-262 (`getFilterTreeByValueItems`: the year, month, day tree of a date column with the number of cells at every node, months and days sorted); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above) and https://github.com/ruilisi/fortune-sheet (`packages/core/src/modules/filter.ts` lines 497-596 (`getFilterColumnColors`: the colour of a cell is its own colour overridden by the colour its conditional format gives it, and a colour list is offered only when the column has two or more); commit 94346608877db4747406707a177c4b8f3bacdbf9, used under the MIT license, Copyright (c) 2022 Suzhou Ruilisi Technology Co., Ltd). `crates/easyoffice-sheet/src/filter/aux.rs` and `dates.rs` take the colour match, the colour list and the date tree; `crates/easyoffice-formats/src/sheet_filter.rs` and `xlsx_filter.rs` take the colour and date group shapes. Modifications: a column holds one colour, as the file does, instead of a list; the colour of a cell is the colour its conditional formats give it (the sheet model has no fill or text colour of its own); the date tree comes from the cell's number split by the engine's own calendar arithmetic, is bounded, and a checked node is a `dateGroupItem` of a unit instead of the checked leaves' texts; a group is judged by one hash lookup per unit. The top-ten, average and 32 date-based `ST_DynamicFilterType` conditions (`crates/easyoffice-sheet/src/filter/dynamic.rs`, PD-238, reusing the day/week/month serial arithmetic of `rules/period.rs` adapted from Univer `packages/sheets-conditional-formatting` and compiling relative periods to an `O(1)` integer day span and recurring quarters/months to a 12-bit `u16` month mask) follow ECMA-376 18.3.2.5 / 18.18.26 and the spreadsheet program's documentation. The ODS filter (`ods_filter.rs`, `ods_tables.rs`) follows the OpenDocument 1.3 schema (9.5.2-9.5.6, 19.688) and takes no code. The reference's worker, panels and rendering are not used.

### IronCalc, rust_xlsxwriter and Univer sheet tables (MIT OR Apache-2.0; MIT OR Apache-2.0; Apache-2.0)

Sources: https://github.com/ironcalc/IronCalc (`base/src/types.rs` lines 426-468 (`Table`, `TableColumn`: name, reference, header and totals row counts, columns, style info, filter flag) and `xlsx/src/import/tables.rs` lines 22-170 (`load_table`: the attributes of a table part and their defaults); commit 87b684bb04a87c63bdd6343c29036450b1cfd358, used under the MIT license, Copyright (c) IronCalc contributors), https://github.com/jmcnamara/rust_xlsxwriter (`src/table.rs` lines 1286-1430 (`assemble_xml_file`, `write_table`, `write_auto_filter`, `write_columns`, `write_table_style_info`: the order of the elements of a table part, `totalsRowShown`, the filter range that leaves out the totals row, the style flags; commit 0c034f85b28e4eaed657ee53a326156b4b03a5d6, used under the MIT license, Copyright 2022-2026, John McNamara) and https://github.com/dream-num/univer (`packages/sheets-filter/src/controllers/sheets-filter.controller.ts` lines 268-518, the range of a filter that follows a row or column insertion or deletion; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the full license text is reproduced in the generated section above). `crates/easyoffice-formats/src/sheet_table.rs`, `xlsx_tables.rs` and `ods_tables.rs` take the model and the shape of a table part in safe Rust; `crates/easyoffice-sheet/src/axis/tables.rs` takes the rules for a range that follows an edit. Modifications: no differential format ids, calculated column formulas or totals functions are modelled; the conditions of a table's filter are read and written through the auto filter reader and writer of the sheet; the column names of a written table come from the header cells, whose text the writer repairs when it is empty, repeated or a formula (the reference writer leaves that to its caller); a part is read with the event stream under bounds for the number of tables and columns of the workbook, and a table that breaks a bound or overlaps another is turned away; the reference importer's sheet-wide table registry, the style engine and the reference writer's worksheet types are not used. An ODS database range is the same model through `table:database-range`, with the totals row as `calcext:contains-footer` as the open-source office suite writes it (behaviour only, no code is taken).

### IronCalc and Univer structured references to tables (MIT OR Apache-2.0; Apache-2.0)

Sources: https://github.com/ironcalc/IronCalc (`base/src/expressions/lexer/structured_references.rs` lines 1-188 (the grammar `table "[" arguments "]"`, the five `#` items, the escape `'` before `[`, `]`, `#`, `'` and `@`; the `@` of a screen is left out), `base/src/expressions/parser/mod.rs` lines 1189-1370 (the rows each item selects: the data rows without an item, `#Totals` of a table with no totals row is `#REF!`, `#This Row` is the formula's own row, a column range from two column names, a reference of one cell), `base/src/expressions/parser/tests/test_tables.rs` (the cases `tblIncome[[#This Row],[Jan]:[Dec]]`, `SUBTOTAL(109, tblIncome[Jan])` and the same table read from another sheet); commit 01b39c1ad26252f7b6c810440e8433a932daee07, used under the MIT license, Copyright (c) IronCalc contributors) and https://github.com/dream-num/univer (`packages/engine-formula/src/engine/reference-object/table-reference-object.ts` lines 43-120 (the rows of each section and `#This Row` as the formula's row), 165-290 (a body split into a section and a column or column range at the first comma that is not inside brackets, one layer of brackets around each part, the `#` words of the sections compared without case), 292-379 (a column or a column range from names, the escape set `['#[]@]`) and `packages/engine-formula/src/functions/lookup/indirect/index.ts`; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd.; the project has no NOTICE file at that commit, the full license text is reproduced in the generated section above). `crates/easyoffice-formats/src/structured_ref.rs` takes the grammar, the escapes and the rows each item selects; `crates/easyoffice-sheet/src/table_index.rs` takes the decision to resolve a reference from the table list and the formula's own cell; `crates/easyoffice-sheet/src/parser.rs` and `table_index.rs` (`join_colon_range`, PD-234) join two resolved endpoint references across the binary `:` range operator (`Table1[a]:Table1[b]`, `[@a]:[@b]`) into their bounding rectangle on the same sheet; `crates/easyoffice-sheet/src/range_refs.rs` (PD-234) resolves structured references (`Table1[Q1]`, `Table1[a]:Table1[b]`, `[@Q1]`, `Table1`) inside `INDIRECT` through the same table index. Modifications: both references read a malformed reference leniently (an unknown `#` word, a second item or a missing column falls back to the data area or to a column position of -1) and the first one turns the reference into a range while it parses, which loses the text; here the syntax is strict, an item list is one the spreadsheet program allows (`#Headers` with `#Data`, `#Data` with `#Totals`, the three together as `#All`), the text of the formula stays as written and is resolved when the formula is compiled, table and column lookups fold ASCII on a 256-byte stack buffer into a non-cryptographic `FastMap` without heap allocation, `INDIRECT` splits `:` outside balanced brackets and quotes under `MAX_FORMULA_LEN`, the nesting of brackets is bounded and nothing recurses, a table or column that is not there or a cross-sheet `:` mismatch is `#REF!` and `#This Row` outside the data rows is `#VALUE!`, and the same scan writes the references again for a file (`Table1[[#This Row],[Q1]]` for `[@Q1]`), for a format that has no tables (the cells they stand for) and for a renamed column. The references' table models, parser state, tokens and worksheet types are not used.

### IronCalc and Apache POI spreadsheet cell colours (MIT OR Apache-2.0; Apache-2.0)

Sources: https://github.com/ironcalc/IronCalc (`base/src/colors.rs` lines 125-165 (the 64-entry default indexed palette), `xlsx/src/import/styles.rs` lines 15-45 (the `indexedColors` override as ARGB or RGB, a malformed entry keeps the default) and 217-245 (a fill takes `fgColor`, then `bgColor`); commit 53c00566515e3d67bf9b7bd2205db16771ea66e8, used under the MIT license) and https://github.com/apache/poi (`poi/src/main/java/org/apache/poi/hssf/record/PaletteRecord.java` lines 35-47 and 95-128 (the 56 palette entries from colour index 8 and the replacement of an entry), `record/ExtendedFormatRecord.java` lines 106-156 (the bit fields of an XF) and `usermodel/HSSFPalette.java` lines 115-132 (`findSimilarColor`); commit 47dcff769bb395034dddfd59c4609659b6dbc6a4, Apache License 2.0, Copyright The Apache Software Foundation; the full license text is reproduced in the generated section above). `crates/easyoffice-formats/src/xlsx_color.rs` ports the default indexed palette, the override of its entries by `indexedColors` (limited to 64 entries) and the fill colour order; `xls_styles.rs` ports the BIFF8 default palette, the XF fill and font fields, the palette entry replacement and the closest-colour search (the sum of the channel differences). Modifications: the palette is a constant of packed RGB, index 64 is automatic and 65 white, an out-of-range number or a tint outside -1..1 is no colour, every read is length checked and bounded (65,536 fills, fonts and formats, 56 palette entries), tint is the float HLS luminance shift, the XLS writer gives a colour its default entry, then a free entry, then the closest one. The Univer cell background and font renderers (`packages/engine-render/src/components/sheets/extensions/background.ts`, `font.ts`; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0) were read for comparison only and no code was taken.

### Apache POI and Univer sheet table styles and table creation (Apache-2.0; Apache-2.0)

Sources: https://github.com/apache/poi (`poi-ooxml/src/main/resources/org/apache/poi/xssf/usermodel/presetTableStyles.xml`, the 60 `TableStyle*` entries of 144 (5,596 of 18,071 lines: the differential formats and the elements that name them, counted from 1), `poi/src/main/java/org/apache/poi/ss/usermodel/TableStyleType.java` lines 41-250 (the elements of a table style in the order they are applied and the cells each covers, stripes counted from the first row below the header with the size of the element) and `poi-ooxml/src/main/java/org/apache/poi/xssf/usermodel/XSSFTableStyle.java` lines 41-95 (an element names a differential format by `dxfId` and has a stripe `size`); commit 47dcff769bb395034dddfd59c4609659b6dbc6a4, Apache License 2.0, Copyright The Apache Software Foundation; the license text is reproduced in the generated section above) and https://github.com/dream-num/univer (`packages/sheets-table/src/commands/commands/add-sheet-table.command.ts` lines 54-70, the default name of a new table: one past the number of tables and one past the biggest number a name with the prefix ends in; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd., same entry as above). `crates/easyoffice-app/ui/sheet-table-presets.js` holds the 60 built-in styles as one record per element (fill, text colour, bold, italic, stripe size, six border edges) with theme colours as slot and tint, `sheet-table-style.js` takes the element order and regions into `covering` and `stripeAt` and answers them for one cell, `crates/easyoffice-formats/src/table_style.rs` reads a file's `tableStyles` with the `dxf` entries the elements name and writes them again, and `sheet-table-ops.js` numbers the default table name. Modifications: the 5,596 lines of XML become 60 lines of records; a cell draws its top and left edges only on the table's first row and column and takes the shared bottom and right edge from the highest element that names it; colours are resolved to RGB (a custom style when it is read, a built-in one with the active workbook's 12-colour `xl/theme/theme1.xml` `clrScheme` when present or the default Office theme, PD-242); `crates/easyoffice-app/ui/sheet-table-style-ui.js` (PD-242) edits the 13 `StyleElement` formats and stripe sizes 1-9 with a 5x5 live preview; `crates/easyoffice-app/ui/sheet-table-ops.js` (`planDedupe`) and `sheet-table-dedupe.js` (PD-242) adapt the typed equality key of IronCalc `base/src/functions/spill_functions.rs` (`fn_unique`, commit 6dc9e8bae4b8d066276f399de7b3e1cf991c220b, MIT OR Apache-2.0) into a single-pass O(R * K) 32-bit FNV-1a bucket scan over selected table columns; `crates/easyoffice-formats/src/xlsx_slicers.rs` and `crates/easyoffice-app/ui/sheet-slicer.js` (PD-242) adapt the per-column table filter model of Univer `packages/sheets-table/src/models/table-filter.ts` (commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache-2.0) to interactive floating slicer cards and `[MS-XLSX]` `slicerCacheDefinition`/`slicer` import plus `<extLst>` round-trip within explicit bounds; the table reader limits a workbook to 1,000 styles of 13 elements with stripes of 1-9; the Java objects, cell range classes and XML bean types are not used, and the reference engine's own six-colour table themes, commands and renderers are not used.

### Univer table filter buttons, IronCalc and Apache POI totals row (Apache-2.0; MIT OR Apache-2.0; Apache-2.0)

Sources: https://github.com/dream-num/univer (`packages/sheets-table/src/models/table-filter.ts` lines 27-82 (`TableFilters`: one condition per column of the table and the set of rows the conditions filter out, replaced as a whole when a column's condition changes), `packages/sheets-table/src/commands/commands/set-table-filter.command.ts` lines 28-66 (a column's filter change is one undoable command that restores the previous condition and rows), `packages/sheets-table-ui/src/controllers/sheet-table-filter-button-render.controller.ts` lines 165-215 (a button on each header cell of a table that has filter buttons, its state taken from the table's filter) and `packages/sheets-table/src/commands/commands/delete-sheet-table.command.ts` lines 25-106 (read for the conversion of a table to a range: it removes the table and its theme and does not write the style into the cells); commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9, Apache License 2.0, Copyright 2023-present DreamNum Co., Ltd., same entry as above), https://github.com/ironcalc/IronCalc (`base/src/functions/subtotal.rs` lines 13-35 (`SubTotalMode`, `CellTableStatus` and the rule for hidden and filtered rows), 58-76 (`cell_hidden_status`), 126-132 (the two `continue`s of the row loop) and 200-225 (the function number table), `base/src/types.rs` lines 267-275 (`Row` with its `hidden` flag, read for the model, not copied) and `xlsx/src/import/tables.rs` lines 10-17 and 140-162 (`totalsRowFunction` and `totalsRowLabel` of a `tableColumn`, which the reference does not read); commit 6dc9e8bae4b8d066276f399de7b3e1cf991c220b, used under the MIT license or the Apache License 2.0, Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH) and https://github.com/apache/poi (`poi-ooxml/src/main/java/org/apache/poi/xssf/usermodel/XSSFTable.java` lines 496-521 and 904-921 (the totals row count and the least rows of a table); commit 47dcff769bb395034dddfd59c4609659b6dbc6a4, Apache License 2.0, Copyright The Apache Software Foundation; the license text is reproduced in the generated section above). `crates/easyoffice-app/ui/sheet-filter.js` (`viewOf`, `store`, `install`: the filter of a table and the sheet's filter are judged, applied and undone by the same commands) and `sheet-grid.js` (the header cells of a table with buttons) take the Univer model and button rule; `crates/easyoffice-formats/src/xlsx_table_totals.rs` writes the `totalsRowFunction`, `totalsRowLabel` and `totalsRowFormula` of ECMA-376 18.5.1.3 and 18.18.80. Modifications: no code of these projects is copied, the page's filter commands, the engine's `filter` action and the cell model are this project's own. The totals functions are derived from the totals-row cells when the file is written (a `SUBTOTAL` over the column with the function number the list writes is that function, any other formula is `custom` with its text, a text is the label) because neither project writes them; `crates/easyoffice-sheet/src/hidden_rows.rs` takes `SubTotalMode`, `CellTableStatus` (as `RowStatus`), the two `continue`s (as `SubtotalMode::skips`) and the function number table (as `subtotal_mode`) with these changes: the reference counts the hidden rows for 110 and 111 although the function numbers above 100 skip them, and the table here skips them; the rows are answered by a binary search over sorted runs the page sends (the reference scans `Worksheet::rows` for each cell and takes a hidden row for a filtered one when a table with filters holds it, here the page says which kind), and the sums and deviations stay in this engine's single-pass accumulator. The conversion of a table to a range writes the table style into the cells' own formats, which none of the projects does.


### rust_xlsxwriter DrawingML chart, pivot table, worksheet drawing, image, comment, and VML note schemas (MIT OR Apache-2.0)

Source: https://github.com/jmcnamara/rust_xlsxwriter (`src/chart.rs`, `src/drawing.rs`, `src/image.rs`, `src/comment.rs`, `src/note.rs`, `src/vml.rs`, `src/content_types.rs`, `src/pivot_table.rs`, `src/pivot_cache.rs`: OOXML `c:chartSpace` / `c:plotArea` / `c:barChart` / `c:lineChart` / `c:pieChart` / `c:scatterChart` / `c:catAx` / `c:valAx` element layouts, `xdr:twoCellAnchor` / `xdr:graphicFrame` / `xdr:pic` drawing parts, DrawingML chart/image/drawing/comments/vmlDrawing relationship types, `xl/media/*` image deduplication and `[Content_Types].xml` defaults, `xl/comments*.xml` author/rich-text layout, VML `ObjectType="Note"` `<v:shape>` / `<x:ClientData>` visibility schema, and `pivotCacheDefinition` / `pivotCacheRecords` / `pivotTableDefinition` / `pivotCaches` schemas; used under MIT OR Apache-2.0, Copyright 2022-2026 John McNamara; full license texts are reproduced above). `crates/easyoffice-formats/src/sheet_chart.rs` and `crates/easyoffice-formats/src/xlsx_charts.rs` (`PD-254`), `crates/easyoffice-formats/src/sheet_pivot.rs` and `crates/easyoffice-formats/src/xlsx_pivots.rs` (`PD-255`), and `crates/easyoffice-formats/src/sheet_media_notes.rs`, `crates/easyoffice-formats/src/xlsx_drawings.rs`, and `crates/easyoffice-formats/src/xlsx_comments.rs` (`PD-256`) adapt these OOXML schemas into safe Rust against this repository's streaming `XmlReader`, `XmlSanitizer::validate_safe_xml`, `FastU64Map` single-pass cell index, packed `u64` coordinate `BTreeMap` note index, bidirectional XLSX round-trip reader/writer, and fail-closed resource budgets (`MAX_CHARTS_PER_SHEET`, `MAX_WORKBOOK_CHARTS`, `MAX_SERIES_PER_CHART`, `MAX_POINTS_PER_SERIES`, `MAX_CHART_TEXT_BYTES`, `MAX_PIVOTS_PER_SHEET`, `MAX_WORKBOOK_PIVOTS`, `MAX_PIVOT_AXIS_FIELDS`, `MAX_PIVOT_VALUE_FIELDS`, `MAX_PIVOT_SLICERS`, `MAX_PIVOT_FILTER_VALUES`, `MAX_PIVOT_TEXT_BYTES`, `MAX_SHEET_IMAGES_PER_SHEET`, `MAX_WORKBOOK_SHEET_IMAGES`, `MAX_WORKBOOK_SHEET_IMAGE_BYTES`, `MAX_NOTES_PER_SHEET`, `MAX_WORKBOOK_NOTES`, `MAX_NOTE_AUTHOR_BYTES`, `MAX_SHEET_NOTE_TEXT_BYTES`). No code of the reference's workbook, worksheet, or XML writer crates is copied.

### qpdf and pdfcpu PDF page labels, name and number tree walk, attachments, annotation matrix (Apache-2.0; Apache-2.0)

Sources: https://github.com/qpdf/qpdf at `4eba9589` (Copyright (c) 2005-2021 Jay Berkenbilt, 2022-2026 Jay Berkenbilt and Manfred Holger) and https://github.com/pdfcpu/pdfcpu at `a21a372d` (Copyright 2026 The pdfcpu Authors). Both are Apache-2.0; their license texts travel in their repositories. The PDF studio (PD-266) carries page labels, attachments and free text, ink and stamp annotations into the file it saves, and each Rust file below names in its leading comment the functions and line ranges it translates. Nothing is linked; the algorithms were translated to Rust on the PDF reader and writer crates the shell already uses.

| EasyOffice file | Source (functions, lines of the source) | Source lines translated | Change |
| --- | --- | --- | --- |
| `crates/easyoffice-desktop/src/pdf_edit_tree.rs` | qpdf `libqpdf/NNTree.cc`: `NNTreeIterator::getNextKid` 76-90, `increment` 91-128, `deepen` 474-543, `NNTreeImpl::begin` 545-551 | 7+15+38+70 = 130 | Forward walk only, as a Rust iterator; the loop check counts the path; loops, depth, node count and the number of kids and items are refused (the source has no such limits); `deepen` goes to the first kid that is a node instead of giving up. Rust: about 75 lines translated, about 15 adapted |
| `crates/easyoffice-desktop/src/pdf_edit_labels.rs` | qpdf `libqpdf/QPDFPageLabelDocumentHelper.cc`: `validate` 32-41, `getLabelForPage` 49-74, `getLabelsForPageRange` 76-123; `libqpdf/QPDFJob.cc` `handlePageSpecs` 2582-2650 (the label calls, about 8 lines) | 10+26+48+8 = 92 | A page without a label gets `/S /D` (the source leaves the style out); numbers past the range are refused. Rust: about 75 lines translated, about 25 adapted |
| `crates/easyoffice-desktop/src/pdf_edit_attach.rs` | qpdf `libqpdf/QPDFEmbeddedFileDocumentHelper.cc`: `validate` 62-78, `getEmbeddedFiles` 119-129; `libqpdf/NNTree.cc` `compareKeys` 561-575; pdfcpu `pkg/pdfcpu/model/name_tree_keys.go`: `uniqueNameTreeEntry` 232-252, `entry.renamed` 254-264 | 17+11+15+21+11 = 75 | The source of the name rule is pdfcpu (a taken name gets `\x01`, or `\x00\x01` for UTF-16BE); qpdf refuses a duplicate. Rust: about 30 lines translated, about 30 adapted |
| `crates/easyoffice-desktop/src/pdf_edit_annots.rs` | qpdf `libqpdf/QPDFMatrix.cc`: constructors 6-24, `concat` 63-78, `translate` 86-90, `transform` 111-116, `transformRectangle` 118-132; `libqpdf/QPDFAcroFormDocumentHelper.cc` `transformAnnotations` 1095 and 1164-1165 | 19+16+5+6+15+3 = 64 | Only the translation to the crop box origin is applied; the source's change of the appearance stream matrix (1146-1160) is not translated because the page copier owns those objects. Rust: about 56 lines translated, about 36 adapted |

Searched and not usable: qpdf keeps no `/OCProperties` of its own when pages are copied (`TODO-pages.md` lists it as open) and has no writer for it; pdfcpu reads it for validation (`pkg/pdfcpu/validate/optionalContent.go`) and adds one watermark layer to an existing list (`pkg/pdfcpu/stamp.go` 1165-1240) but does not combine the lists of two files (`pkg/pdfcpu/merge.go` has page labels and layers nowhere); the other candidates read it for display only or do not read the catalog at all. `pdf_edit_oc.rs`, `pdf_edit_value.rs` and `pdf_edit_carry.rs` are therefore written here.

### sse-core (MIT OR Apache-2.0, used under MIT)

Source: https://github.com/PizzasBear/sse-rs, crate `sse-core` 0.2.3 (Copyright (c) 2026 Max Shteimberg). `crates/easyoffice-app` depends on it as a crate (`PD-269`; it replaced the earlier port in `copilot/sse.rs`) for the `SseDecoder` state machine of the model response stream. Used under the MIT license:

```
MIT License

Copyright (c) 2026 Max Shteimberg

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

### Spreadsheet exponential smoothing forecast: `FORECAST.ETS` functions (MPL-2.0, one file, to be replaced by a clean-room implementation)

Source: https://github.com/LibreOffice/core at commit `2b66365f1b81b7a89bdc1d93788e4c96073fa513` (LibreOffice Calc), Mozilla Public License, v. 2.0. This is the one exception to the rule that only permissive licences are taken (PD-272, QA decision 2026-11-21): no maintained implementation under a permissive licence exists (IronCalc and Univer have empty functions, the R `forecast` package is GPL-3), so the MPL-2.0 original is used until a clean-room implementation replaces it (**클린룸 대체 예정**, `TODO.md`).

Ported: `sc/source/core/tool/interpr8.cxx` lines 25-1172 (1,148 lines: `DataPoint`, `lcl_SortByX`, the class `ScETSForecastCalculation` with every method: data preparation and aggregation, gap filling, season detection, search of the smoothing coefficients, accuracy figures, forecast, prediction intervals) and `sc/inc/kahan.hxx` lines 46-278 (`KahanSum`). The sources carry no copyright line of their own; the file history names Winfried Donkers as the author of the forecast functions (2016, tdf#94635) and the project's contributors for later changes. The test vectors in `crates/easyoffice-sheet/tests/data/forecast_ets_vectors.txt` are the data and the expected values of `sc/qa/unit/data/functions/statistical/fods/forecast.ets.add.fods` and `forecast.ets.mult.fods` of the same commit (21 cases).

Where it is: the MPL code is the file `crates/easyoffice-sheet/src/functions_forecast_ets.rs` (976 lines, its header holds the notice and the origin) and nothing else; the vector file is data from the same source and carries the same notice. The Source Code Form of both files is in this repository and stays under the MPL-2.0 (Larger Work, MPL-2.0 section 3.3); the rest of the program, which only calls the file through `crates/easyoffice-sheet/src/functions_forecast_ets_args.rs` (own code), stays under its own licence. Parts of the original that carry other licence notices (`interpr3.cxx`, `sal/rtl/math.cxx`: percentile, normal quantile, approximate equality) are not taken; the file has its own short versions. Modifications are marked `CHANGE` in the file: names and types (snake case, slices, a context of services), work and memory limits that fail closed, the random numbers of the prediction interval come from the caller (a fixed seed), the average of equal timeline values is a real average (the original keeps the first value), a month timeline fills gaps in month numbers, the interpolation of a prediction interval between two steps is a fraction of a step, a series with no seasonality takes the closed form interval, an all-zero period average is an error only for the multiplicative model, the three coefficient searches are one function, and `KahanSum` is a plain Neumaier sum.

```
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.
```

## FORECAST.ETS clean-room reference material (documents only, no code)

`crates/easyoffice-sheet/src/functions_forecast_ets_cr.rs` (Apache-2.0) is written from public documents. No code is copied. The record of every rule and choice is `docs/forecast-ets-cleanroom.md`; its sources S1 to S5 are the vendor function documents below.

- Microsoft Excel function documents, read on 2026-11-21: FORECAST.ETS, FORECAST.ETS.CONFINT, FORECAST.ETS.SEASONALITY, FORECAST.ETS.STAT and "Forecasting functions (reference)", at `https://support.microsoft.com/en-us/excel/functions/forecast-ets-function`, `.../forecast-ets-confint-function`, `.../forecast-ets-seasonality-function`, `.../forecast-ets-stat-function`, `.../forecasting-functions-reference`. Used for the argument rules and error conditions only.
- Hyndman and Athanasopoulos, *Forecasting: Principles and Practice*, 3rd ed. (https://otexts.com/fpp3/, CC BY-NC-SA 4.0). Only the published formulas (sections 2.8, 5.8, 8.1, 8.3, 8.5 to 8.7, 9.1) are used; no text is copied.
- statsmodels (BSD-3-Clause, https://www.statsmodels.org). It was run outside the repository with fixed initial states and fixed coefficients to produce reference numbers for the tests (`functions_forecast_ets_cr_tests.rs`). Nothing of it is vendored or linked.
- Kolda, Lewis and Torczon, "Optimization by Direct Search", SIAM Review 45(3), 2003 (doi:10.1137/S0036144502428893), for the shape of the coordinate search.
- Reference values (numbers only, no code): the results Microsoft Excel stored in the example workbook `Forecast_ets_example.xlsx`, which the vendor publishes with the FORECAST.ETS function document above (workbook saved 2016-11, read on 2026-11-21). The monthly airport passenger series (57 points), the 24 forecasts and interval bounds, the detected seasonality 12 and the STAT alpha 0.251 are in `crates/easyoffice-sheet/tests/data/forecast_ets_excel_reference.json`. Nothing else of the workbook is kept. The values are facts computed by the vendor's functions; the tests only compare against them.
