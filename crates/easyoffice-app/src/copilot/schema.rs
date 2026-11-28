//! 모듈별 코파일럿 연산·읽기 도구의 JSON 스키마 정의 및 OpenAI·Anthropic·Google 네이티브 도구 호출 변환기.

use easyoffice_formats::StrictJsonValue;
use std::fmt::Write as _;

use super::cache::{is_official_anthropic_host, is_openrouter_anthropic_cache, uses_native_tools};
use super::types::{push_json_str, CopilotCommand, CopilotModule, CopilotRunRequest, ProviderKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeToolInvocation {
    pub name: String,
    pub arguments_json: String,
}

struct ToolDef {
    name: &'static str,
    description: &'static str,
    parameters_json: &'static str,
}

const READ_DOC_SCHEMA: &str = r#"{"type":"object","properties":{"startIndex":{"type":"integer","minimum":0},"endIndex":{"type":"integer","minimum":0}},"required":["startIndex","endIndex"]}"#;
const READ_SHEET_SCHEMA: &str = r#"{"type":"object","properties":{"sheet":{"type":"string"},"range":{"type":"string"}},"required":["range"]}"#;
const FIND_SHEET_SCHEMA: &str = r#"{"type":"object","properties":{"sheet":{"type":"string"},"query":{"type":"string"}},"required":["query"]}"#;
const SEARCH_PDF_SCHEMA: &str =
    r#"{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}"#;
const READ_BASE_SCHEMA: &str = r#"{"type":"object","properties":{"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":50},"query":{"type":"string"}},"required":["offset","limit"]}"#;
const READ_SLIDES_SCHEMA: &str = r#"{"type":"object","properties":{"start":{"type":"integer","minimum":0},"count":{"type":"integer","minimum":1,"maximum":20}},"required":["start","count"]}"#;

const DOC_BLOCK_ITEM_SCHEMA: &str = r#"{"type":"object","properties":{"heading":{"type":"integer","minimum":1},"text":{"type":"string"},"list":{"type":"string","enum":["bullet","ordered","task"]},"items":{"type":"array","items":{"type":"string"}},"table":{"type":"object","properties":{"headerRow":{"type":"boolean"},"rows":{"type":"array","items":{"type":"array","items":{"type":"string"}}}},"required":["rows"]}}}"#;
const SLIDE_ELEMENT_SCHEMA: &str = r#"{"type":"object","properties":{"id":{"type":"string"},"kind":{"type":"string","enum":["textbox","shape","table","chart","image"]},"x":{"type":"number"},"y":{"type":"number"},"w":{"type":"number"},"h":{"type":"number"},"text":{"type":"string"},"role":{"type":"string","enum":["title","subtitle","body","none"]},"fontSizePt":{"type":"number"},"bold":{"type":"boolean"},"italic":{"type":"boolean"},"color":{"type":"string"},"fill":{"type":"string"},"align":{"type":"string","enum":["left","center","right","justify"]},"shapeType":{"type":"string"},"stroke":{"type":"string"},"rows":{"type":"array","items":{"type":"array","items":{"type":"string"}}},"headerRow":{"type":"boolean"},"chartType":{"type":"string","enum":["column","bar","line","pie","scatter"]},"title":{"type":"string"},"categories":{"type":"array","items":{"type":"string"}},"series":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"values":{"type":"array","items":{"type":"number"}},"color":{"type":"string"}},"required":["values"]}}},"required":["kind"]}"#;

fn op_item_schema(req: &CopilotRunRequest) -> (String, Option<&'static str>) {
    if req.is_read_only() {
        return (
            r#"{"type":"object","properties":{"kind":{"type":"string"}}}"#.to_string(),
            None,
        );
    }
    if let Some(cmd) = req.command {
        match req.module {
            CopilotModule::Doc | CopilotModule::Hwp => {
                if cmd == CopilotCommand::VisualizeTable && req.module == CopilotModule::Doc {
                    return (
                        format!(
                            r#"{{"type":"object","properties":{{"kind":{{"type":"string","enum":["doc.replace_paragraphs"]}},"startIndex":{{"type":"integer","minimum":0}},"endIndex":{{"type":"integer","minimum":0}},"blocks":{{"type":"array","items":{DOC_BLOCK_ITEM_SCHEMA}}}}},"required":["kind","startIndex","endIndex","blocks"]}}"#
                        ),
                        None,
                    );
                }
                return (
                    r#"{"type":"object","properties":{"kind":{"type":"string","enum":["doc.replace_paragraphs"]},"replacements":{"type":"array","items":{"type":"object","properties":{"index":{"type":"integer","minimum":0},"text":{"type":"string"}},"required":["index","text"]}}},"required":["kind","replacements"]}"#.to_string(),
                    None,
                );
            }
            CopilotModule::Slide => {
                return (
                    r#"{"type":"object","properties":{"kind":{"type":"string","enum":["slide.update"]},"index":{"type":"integer","minimum":0},"title":{"type":"string"},"bullets":{"type":"array","items":{"type":"string"}}},"required":["kind"]}"#.to_string(),
                    None,
                );
            }
            _ => {}
        }
    }
    match req.module {
        CopilotModule::Doc => (
            format!(
                r#"{{"type":"object","properties":{{"kind":{{"type":"string","enum":["doc.append","doc.replace_paragraphs","doc.replace_span","doc.scrub_pii"]}},"blocks":{{"type":"array","items":{DOC_BLOCK_ITEM_SCHEMA}}},"replacements":{{"type":"array","items":{{"type":"object","properties":{{"index":{{"type":"integer","minimum":0}},"text":{{"type":"string"}}}},"required":["index","text"]}}}},"startIndex":{{"type":"integer","minimum":0}},"endIndex":{{"type":"integer","minimum":0}},"index":{{"type":"integer","minimum":0}},"search":{{"type":"string"}},"replace":{{"type":"string"}}}},"required":["kind"]}}"#
            ),
            None,
        ),
        CopilotModule::Hwp => (
            r#"{"type":"object","properties":{"kind":{"type":"string","enum":["doc.append","doc.replace_paragraphs","doc.replace_span"]},"blocks":{"type":"array","items":{"type":"object","properties":{"heading":{"type":"integer"},"text":{"type":"string"},"list":{"type":"string"},"items":{"type":"array","items":{"type":"string"}}}}},"replacements":{"type":"array","items":{"type":"object","properties":{"index":{"type":"integer","minimum":0},"text":{"type":"string"}},"required":["index","text"]}},"startIndex":{"type":"integer","minimum":0},"endIndex":{"type":"integer","minimum":0},"index":{"type":"integer","minimum":0},"search":{"type":"string"},"replace":{"type":"string"}},"required":["kind"]}"#.to_string(),
            None,
        ),
        CopilotModule::Sheet => (
            r#"{"type":"object","properties":{"kind":{"type":"string","enum":["sheet.write","sheet.format","sheet.chart","sheet.table","sheet.pivot","sheet.cond_format","sheet.sort_filter","sheet.structure","sheet.note"]},"sheet":{"type":"string"},"range":{"type":"string"},"cells":{"type":"array","items":{"type":"object","properties":{"coord":{"type":"string"},"raw":{"type":"string"},"numFmt":{"type":"string"},"bold":{"type":"boolean"},"fill":{"type":"string"},"color":{"type":"string"}},"required":["coord","raw"]}},"fill":{"type":"string"},"color":{"type":"string"},"bold":{"type":"boolean"},"numFmt":{"type":"string"},"chartType":{"type":"string","enum":["column","bar","line","pie","scatter"]},"dataRange":{"type":"string"},"title":{"type":"string"},"legend":{"type":"string","enum":["right","bottom","top","left","none"]},"switchRowCol":{"type":"boolean"},"action":{"type":"string"},"tableName":{"type":"string"},"style":{"type":"string"},"totalRow":{"type":"boolean"},"sourceRange":{"type":"string"},"destSheet":{"type":"string"},"destCell":{"type":"string"},"rowField":{"type":"string"},"colField":{"type":"string"},"valueField":{"type":"string"},"agg":{"type":"string","enum":["sum","count","average","min","max"]},"preset":{"type":"string"},"value1":{"type":"string"},"value2":{"type":"string"},"colOffset":{"type":"integer"},"ascending":{"type":"boolean"},"query":{"type":"string"},"index":{"type":"integer"},"count":{"type":"integer"},"coord":{"type":"string"},"text":{"type":"string"}},"required":["kind"]}"#.to_string(),
            None,
        ),
        CopilotModule::Slide => (
            r##"{"type":"object","properties":{"kind":{"type":"string","enum":["slide.add","slide.update","slide.add_element"]},"index":{"type":"integer","minimum":0},"slideIndex":{"type":"integer","minimum":0},"title":{"type":"string"},"body":{"type":"string"},"bullets":{"type":"array","items":{"type":"string"}},"notes":{"type":"string"},"layout":{"type":"string","enum":["titleSlide","titleAndContent","sectionHeader","twoColumn","blank"]},"transition":{"type":"string","enum":["none","fade","push","wipe","split","zoom"]},"elements":{"type":"array","items":{"$ref":"#/$defs/SlideEl"}},"element":{"$ref":"#/$defs/SlideEl"}},"required":["kind"]}"##.to_string(),
            Some(SLIDE_ELEMENT_SCHEMA),
        ),
        CopilotModule::Base => (
            r#"{"type":"object","properties":{"kind":{"type":"string","enum":["base.add_field","base.add_records","base.update_records","base.set_view"]},"name":{"type":"string"},"fieldType":{"type":"string","enum":["text","number","checkbox","select","date"]},"options":{"type":"array","items":{"type":"string"}},"records":{"type":"array","items":{"type":"object"}},"updates":{"type":"array","items":{"type":"object","properties":{"rowIndex":{"type":"integer","minimum":0},"values":{"type":"object"}},"required":["rowIndex","values"]}},"view":{"type":"string","enum":["grid","kanban","calendar"]},"filterField":{"type":"string"},"filterOp":{"type":"string","enum":["contains","eq","neq","gt","gte","lt","lte","empty","notEmpty"]},"filterValue":{"type":"string"},"sortField":{"type":"string"},"sortDir":{"type":"string","enum":["asc","desc"]},"groupField":{"type":"string"}},"required":["kind"]}"#.to_string(),
            None,
        ),
        CopilotModule::Board => (
            r#"{"type":"object","properties":{"kind":{"type":"string","enum":["board.add_graph"]},"direction":{"type":"string","enum":["TB","LR"]},"nodes":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"kind":{"type":"string","enum":["shape","text"]},"shape":{"type":"string","enum":["rect","roundRect","diamond","ellipse"]},"text":{"type":"string"},"fill":{"type":"string"}},"required":["id","text"]}},"edges":{"type":"array","items":{"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"label":{"type":"string"},"style":{"type":"string","enum":["orthogonal","curved"]}},"required":["from","to"]}}},"required":["kind","nodes"]}"#.to_string(),
            None,
        ),
        CopilotModule::Pdf => (
            r#"{"type":"object","properties":{"kind":{"type":"string"}}}"#.to_string(),
            None,
        ),
    }
}

fn build_emit_schema(req: &CopilotRunRequest) -> String {
    let (op_schema, slide_def) = op_item_schema(req);
    let defs = match slide_def {
        Some(el_schema) => format!(r#""$defs":{{"SlideEl":{el_schema}}},"#),
        None => String::new(),
    };
    format!(
        r#"{{"type":"object",{defs}"properties":{{"reply":{{"type":"string"}},"operations":{{"type":"array","items":{op_schema}}}}},"required":["reply","operations"]}}"#
    )
}

fn read_tools_for_module(req: &CopilotRunRequest) -> Vec<ToolDef> {
    if req.lightweight || req.command.is_some() {
        return Vec::new();
    }
    match req.module {
        CopilotModule::Doc | CopilotModule::Hwp => vec![ToolDef {
            name: "read_doc_blocks",
            description: "지정 범위의 문단을 추가로 읽는다.",
            parameters_json: READ_DOC_SCHEMA,
        }],
        CopilotModule::Sheet => vec![
            ToolDef {
                name: "read_sheet_range",
                description: "지정한 셀 범위의 수식과 값을 읽는다.",
                parameters_json: READ_SHEET_SCHEMA,
            },
            ToolDef {
                name: "find_sheet_cells",
                description: "시트에서 검색어가 들어 있는 셀을 찾는다.",
                parameters_json: FIND_SHEET_SCHEMA,
            },
        ],
        CopilotModule::Pdf => vec![ToolDef {
            name: "search_pdf_pages",
            description: "PDF 전체 쪽에서 검색어가 있는 쪽과 발췌문을 찾는다.",
            parameters_json: SEARCH_PDF_SCHEMA,
        }],
        CopilotModule::Base => vec![ToolDef {
            name: "read_base_records",
            description: "현재 베이스 테이블의 레코드를 오프셋과 검색어로 추가 조회한다.",
            parameters_json: READ_BASE_SCHEMA,
        }],
        CopilotModule::Slide => vec![ToolDef {
            name: "read_slides",
            description: "지정한 번호부터 슬라이드의 제목·항목·노트·개체를 모두 읽는다. 문맥에는 활성 슬라이드만 자세히 있다.",
            parameters_json: READ_SLIDES_SCHEMA,
        }],
        _ => Vec::new(),
    }
}

pub fn has_emitted_tools(req: &CopilotRunRequest) -> bool {
    if req.lightweight {
        return false;
    }
    let read_only = req.is_read_only();
    let read_tools = read_tools_for_module(req);
    !(read_only && read_tools.is_empty())
}

/// 공식 제공자(`OpenAI`·`Anthropic`·`Google`·공식 `DeepSeek`/`OpenRouter`/`xAI`) 요청 본문에 네이티브 `tools` 스키마를 덧붙인다.
/// `lightweight` 요청은 도구를 생략하고, 읽기 전용(`ask`·`PDF`)은 필요한 읽기 도구만 남기며 0개면 `tools` 자체를 생략한다.
pub fn write_provider_tools_field(out: &mut String, req: &CopilotRunRequest) {
    if req.lightweight || !uses_native_tools(req) {
        return;
    }
    let read_only = req.is_read_only();
    let read_tools = read_tools_for_module(req);
    if read_only && read_tools.is_empty() {
        return;
    }
    let emit_schema = (!read_only).then(|| build_emit_schema(req));
    match req.provider.kind {
        ProviderKind::OpenAi
        | ProviderKind::DeepSeek
        | ProviderKind::OpenRouter
        | ProviderKind::Xai => {
            let allow_cache = is_openrouter_anthropic_cache(req);
            let total_tools = usize::from(emit_schema.is_some()) + read_tools.len();
            let mut emitted = 0usize;
            out.push_str(",\"tools\":[");
            if let Some(schema) = &emit_schema {
                out.push_str("{\"type\":\"function\",\"function\":{\"name\":\"emit_copilot_turn\",\"description\":\"최종 답변과 편집 작업 목록을 구조화해 반환한다.\",\"parameters\":");
                out.push_str(schema);
                out.push('}');
                emitted += 1;
                if allow_cache && emitted == total_tools {
                    out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
                }
                out.push('}');
            }
            for t in &read_tools {
                if emitted > 0 {
                    out.push(',');
                }
                let _ = write!(
                    out,
                    "{{\"type\":\"function\",\"function\":{{\"name\":\"{}\",\"description\":",
                    t.name
                );
                push_json_str(out, t.description);
                out.push_str(",\"parameters\":");
                out.push_str(t.parameters_json);
                out.push('}');
                emitted += 1;
                if allow_cache && emitted == total_tools {
                    out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
                }
                out.push('}');
            }
            out.push(']');
        }
        ProviderKind::Anthropic => {
            let allow_cache =
                req.command.is_none() && is_official_anthropic_host(&req.provider.base_url);
            let total_tools = usize::from(emit_schema.is_some()) + read_tools.len();
            let mut emitted = 0usize;
            out.push_str(",\"tools\":[");
            if let Some(schema) = &emit_schema {
                out.push_str("{\"name\":\"emit_copilot_turn\",\"description\":\"최종 답변과 편집 작업 목록을 구조화해 반환한다.\",\"input_schema\":");
                out.push_str(schema);
                emitted += 1;
                if allow_cache && emitted == total_tools {
                    out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
                }
                out.push('}');
            }
            for t in &read_tools {
                if emitted > 0 {
                    out.push(',');
                }
                let _ = write!(out, "{{\"name\":\"{}\",\"description\":", t.name);
                push_json_str(out, t.description);
                out.push_str(",\"input_schema\":");
                out.push_str(t.parameters_json);
                emitted += 1;
                if allow_cache && emitted == total_tools {
                    out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
                }
                out.push('}');
            }
            out.push(']');
        }
        ProviderKind::Google => {
            out.push_str(",\"tools\":[{\"functionDeclarations\":[");
            let mut wrote = false;
            if let Some(schema) = &emit_schema {
                out.push_str("{\"name\":\"emit_copilot_turn\",\"description\":\"최종 답변과 편집 작업 목록을 구조화해 반환한다.\",\"parametersJsonSchema\":");
                out.push_str(schema);
                out.push('}');
                wrote = true;
            }
            for t in &read_tools {
                if wrote {
                    out.push(',');
                }
                let _ = write!(out, "{{\"name\":\"{}\",\"description\":", t.name);
                push_json_str(out, t.description);
                out.push_str(",\"parametersJsonSchema\":");
                out.push_str(t.parameters_json);
                out.push('}');
                wrote = true;
            }
            out.push_str("]}]");
        }
        _ => {}
    }
}

/// 모델이 네이티브 도구 호출(`tool_calls`·`tool_use`·`functionCall`)을 반환했을 때 표준 응답 JSON으로 합친다.
pub fn synthesize_completion_from_tools(
    text_out: String,
    invocations: &[NativeToolInvocation],
) -> String {
    if invocations.is_empty() {
        return text_out;
    }
    let mut reply = String::new();
    let mut reasoning = String::new();
    let mut ops_raw: Vec<String> = Vec::new();
    let mut locs_raw: Vec<String> = Vec::new();
    let mut read_calls_raw: Vec<String> = Vec::new();

    for inv in invocations {
        let parsed = StrictJsonValue::parse(&inv.arguments_json).ok();
        let Some(obj) = parsed.as_ref().and_then(StrictJsonValue::as_object) else {
            continue;
        };
        if inv.name == "emit_copilot_turn" {
            if let Some(r) = StrictJsonValue::get_str(obj, "reply") {
                if !r.is_empty() {
                    reply = r.to_string();
                }
            }
            if let Some(th) = StrictJsonValue::get_str(obj, "reasoning") {
                if !th.is_empty() {
                    reasoning = th.to_string();
                }
            }
            if let Some(arr) =
                StrictJsonValue::get_field(obj, "operations").and_then(StrictJsonValue::as_array)
            {
                for item in arr {
                    ops_raw.push(item.to_json_string());
                }
            }
            if let Some(arr) =
                StrictJsonValue::get_field(obj, "locations").and_then(StrictJsonValue::as_array)
            {
                for item in arr {
                    locs_raw.push(item.to_json_string());
                }
            }
            if let Some(arr) =
                StrictJsonValue::get_field(obj, "toolCalls").and_then(StrictJsonValue::as_array)
            {
                for item in arr {
                    read_calls_raw.push(item.to_json_string());
                }
            }
        } else if matches!(
            inv.name.as_str(),
            "read_doc_blocks"
                | "read_hwp_paragraphs"
                | "read_sheet_range"
                | "find_sheet_cells"
                | "search_pdf_pages"
                | "read_base_records"
                | "read_slides"
        ) {
            let mut entries = obj.to_vec();
            entries.push(("name".to_string(), StrictJsonValue::Str(inv.name.clone())));
            read_calls_raw.push(StrictJsonValue::Object(entries).to_json_string());
        } else if inv.name.contains('.') {
            let mut entries = obj.to_vec();
            entries.push(("kind".to_string(), StrictJsonValue::Str(inv.name.clone())));
            ops_raw.push(StrictJsonValue::Object(entries).to_json_string());
        }
    }

    if reply.is_empty() {
        let trimmed = text_out.trim();
        if let Ok(StrictJsonValue::Object(t_obj)) = StrictJsonValue::parse(trimmed) {
            if let Some(r) = StrictJsonValue::get_str(&t_obj, "reply") {
                reply = r.to_string();
            }
        }
        if reply.is_empty() {
            reply = trimmed.to_string();
        }
    }

    let mut out = String::with_capacity(256 + reply.len());
    out.push_str("{\"reply\":");
    push_json_str(&mut out, &reply);
    out.push_str(",\"reasoning\":");
    push_json_str(&mut out, &reasoning);
    out.push_str(",\"operations\":[");
    out.push_str(&ops_raw.join(","));
    out.push_str("],\"locations\":[");
    out.push_str(&locs_raw.join(","));
    out.push_str("],\"toolCalls\":[");
    out.push_str(&read_calls_raw.join(","));
    out.push_str("]}");
    out
}
