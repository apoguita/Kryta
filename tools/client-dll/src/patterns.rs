//! Offsets: the JSON documents that say how to find each place in the client, and the resolver that
//! follows them.
//!
//! Ported from Py4GW Reforged Native at `e2f5664`: `src/base/patterns.cpp` (1,033 lines) and
//! `include/base/patterns.h`. The document format is Reforged's, unchanged, so an offset written for
//! one project works in the other (`docs/port/22-client-platform.md`, "Py4GW Reforged, the
//! reference"):
//!
//! ```json
//! {
//!   "namespace": "titles",
//!   "patterns":  { "<name>": { "pattern": "\\x55\\x8B", "mask": "xx", "offset": "0x0", "section": "text" } },
//!   "resolvers": { "<name>": { "steps": [ { "op": "scan", "pattern": "<name>", "out": "final" } ] } }
//! }
//! ```
//!
//! A resolver is a list of steps, or several `attempts` tried in order; each step reads earlier
//! outputs by name and writes one, and the output named `final` is the resolver's answer.
//!
//! Where this differs from Reforged: Reforged reads its documents from a directory beside the DLL;
//! here the server sends them over the link (owner, 2026-11-23), so [`Patterns::load`] takes a
//! parsed document.

use std::collections::HashMap;

use serde_json::Value;

use crate::scanner::{Scanner, Section};

/// One pattern of a document: bytes under a mask, or an assertion to find.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pattern {
    bytes: Vec<u8>,
    mask: String,
    assertion_file: String,
    assertion_message: String,
    offset: i32,
    line_number: i32,
    section: Section,
}

/// The ten things a step can do (`patterns.cpp:26-37`, `ParseStepOp` `:294-343`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Scan,
    ScanInRange,
    ToFunctionStart,
    FunctionFromNearCall,
    FindUseOfString,
    Dereference,
    ReadU32,
    Add,
    Divide,
    ValidateSection,
}

impl Op {
    fn parse(text: &str) -> Option<Self> {
        Some(match text.to_ascii_lowercase().as_str() {
            "scan" | "pattern_scan" | "pattern" => Self::Scan,
            "scan_in_range" | "pattern_scan_in_range" | "find_in_range" => Self::ScanInRange,
            "to_function_start" => Self::ToFunctionStart,
            "function_from_near_call" => Self::FunctionFromNearCall,
            "find_use_of_string" | "use_of_string" | "scan_use_of_string" => Self::FindUseOfString,
            "deref" | "dereference" => Self::Dereference,
            "read_u32" | "read_uint32" => Self::ReadU32,
            "add" => Self::Add,
            "divide" | "div" => Self::Divide,
            "validate_section" => Self::ValidateSection,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::ScanInRange => "scan_in_range",
            Self::ToFunctionStart => "to_function_start",
            Self::FunctionFromNearCall => "function_from_near_call",
            Self::FindUseOfString => "find_use_of_string",
            Self::Dereference => "dereference",
            Self::ReadU32 => "read_u32",
            Self::Add => "add",
            Self::Divide => "divide",
            Self::ValidateSection => "validate_section",
        }
    }
}

/// One step of a resolver (`ResolverStepObject`, `patterns.cpp:39-57`).
#[derive(Debug, Clone)]
struct Step {
    name: String,
    op: Op,
    input: String,
    output: String,
    pattern: String,
    literal: String,
    start: String,
    end: String,
    start_add: i32,
    end_add: i32,
    value: i32,
    nth: i32,
    scan_range: i32,
    check_valid_ptr: bool,
    wide: bool,
    section: Option<Section>,
}

#[derive(Debug, Clone)]
struct Attempt {
    name: String,
    steps: Vec<Step>,
}

#[derive(Debug, Clone)]
struct Resolver {
    attempts: Vec<Attempt>,
    /// `on_fail`: whether the caller may carry on without this address.
    may_continue: bool,
}

/// Reads the running process's memory, for the two steps that follow a pointer.
///
/// Reforged reads live memory behind a `VirtualQuery` check (`CanReadMemory`, `patterns.cpp:75-102`);
/// the DLL supplies that, and a test supplies a table.
pub trait Memory {
    /// The 32-bit value at an address, when it can be read.
    fn read_u32(&self, address: u32) -> Option<u32>;
}

/// How a resolver ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// The address, when the resolver produced one.
    pub value: Option<u32>,
    /// Why not, when it did not: the failed step and its reason.
    pub message: String,
    /// Whether the document lets the caller carry on without this address (`on_fail: continue`).
    pub may_continue: bool,
    /// Each step that ran, as `name op input -> output`, for the log.
    pub trace: Vec<String>,
}

/// Every pattern and resolver the server sent.
#[derive(Debug, Default)]
pub struct Patterns {
    patterns: HashMap<String, Pattern>,
    resolvers: HashMap<String, Resolver>,
}

/// `namespace.name`, unless there is no namespace or the name already has one
/// (`QualifyName`, `patterns.cpp:104-109`).
fn qualify(namespace: &str, name: &str) -> String {
    if namespace.is_empty() || name.contains('.') {
        name.to_string()
    } else {
        format!("{namespace}.{name}")
    }
}

/// `rdata`, `data`, and anything else is `text` (`ParseSection`, `patterns.cpp:119-132`).
fn section(text: &str) -> Section {
    match text.to_ascii_lowercase().as_str() {
        "rdata" => Section::RData,
        "data" => Section::Data,
        _ => Section::Text,
    }
}

/// A whole number written in decimal, hex (`0x`) or octal (leading `0`), with an optional sign, as
/// `std::stoll(text, &parsed, 0)` reads it (`ParseInt`, `patterns.cpp:134-153`).
fn parse_int(text: &str) -> Option<i32> {
    let (negative, digits) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    let magnitude = if let Some(hex) = digits.strip_prefix("0x").or(digits.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()?
    } else if digits.len() > 1 && digits.starts_with('0') {
        i64::from_str_radix(&digits[1..], 8).ok()?
    } else {
        digits.parse::<i64>().ok()?
    };
    i32::try_from(if negative { -magnitude } else { magnitude }).ok()
}

/// A number given as a JSON integer or as text (`ParseIntValue`, `patterns.cpp:238-254`).
fn int_value(value: &Value) -> Option<i32> {
    match value {
        Value::Number(number) => number.as_i64().and_then(|raw| i32::try_from(raw).ok()),
        Value::String(text) => parse_int(text),
        _ => None,
    }
}

/// A flag given as a JSON boolean, an integer, or one of six words (`ParseBool`,
/// `patterns.cpp:209-236`).
fn bool_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::Number(number) => number.as_i64().map(|raw| raw != 0),
        Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// The bytes a pattern's text stands for: `\xNN` is a byte, `\\`, `\0`, `\n`, `\r` and `\t` are
/// themselves, any other escaped character is that character (`DecodePatternLiteral`,
/// `patterns.cpp:155-207`).
fn decode_literal(literal: &str) -> Option<Vec<u8>> {
    let source = literal.as_bytes();
    let mut bytes = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index] != b'\\' {
            bytes.push(source[index]);
            index += 1;
            continue;
        }
        let escape = *source.get(index + 1)?;
        index += 2;
        match escape {
            b'\\' => bytes.push(b'\\'),
            b'0' => bytes.push(0),
            b'n' => bytes.push(b'\n'),
            b'r' => bytes.push(b'\r'),
            b't' => bytes.push(b'\t'),
            b'x' => {
                let digits = std::str::from_utf8(source.get(index..index + 2)?).ok()?;
                bytes.push(u8::from_str_radix(digits, 16).ok()?);
                index += 2;
            }
            other => bytes.push(other),
        }
    }
    Some(bytes)
}

fn text_of<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or(default)
}

impl Patterns {
    /// No patterns yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one document's patterns and resolvers.
    ///
    /// Port of `LoadFile` (`patterns.cpp:583-651`) and `LoadResolvers` (`:519-581`): the same
    /// fields, the same defaults and the same refusals, with the refusal returned to the caller
    /// instead of written to Reforged's log.
    pub fn load(&mut self, document: &Value) -> Result<(), String> {
        let namespace = text_of(document, "namespace", "");

        if let Some(patterns) = document.get("patterns").and_then(Value::as_object) {
            for (raw_name, value) in patterns {
                let name = qualify(namespace, raw_name);
                if self.patterns.contains_key(&name) {
                    return Err(format!("Duplicate pattern entry: {name}"));
                }
                let invalid = || format!("Invalid pattern entry: {name}");
                let literal = text_of(value, "pattern", "");
                let bytes = if literal.is_empty() {
                    Vec::new()
                } else {
                    decode_literal(literal).ok_or_else(invalid)?
                };
                let number = |key: &str| -> Result<i32, String> {
                    match value.get(key) {
                        None => Ok(0),
                        Some(field) => int_value(field).ok_or_else(invalid),
                    }
                };
                let offset = number("offset")?;
                let line_number = number("line_number")?;
                number("range")?;
                let has = |key: &str| value.get(key).is_some();
                if !["pattern", "mask", "assertion_file", "assertion_message"]
                    .iter()
                    .chain(&["offset", "line_number", "range", "section"])
                    .any(|key| has(key))
                {
                    return Err(format!(
                        "Pattern entry has no usable scanner inputs: {name}"
                    ));
                }
                self.patterns.insert(
                    name,
                    Pattern {
                        bytes,
                        mask: text_of(value, "mask", "").to_string(),
                        assertion_file: text_of(value, "assertion_file", "").to_string(),
                        assertion_message: text_of(value, "assertion_message", "").to_string(),
                        offset,
                        line_number,
                        section: section(text_of(value, "section", "text")),
                    },
                );
            }
        }

        let Some(resolvers) = document.get("resolvers").and_then(Value::as_object) else {
            return Ok(());
        };
        for (raw_name, value) in resolvers {
            let name = qualify(namespace, raw_name);
            if self.resolvers.contains_key(&name) {
                return Err(format!("Duplicate resolver entry: {name}"));
            }
            if !value.is_object() {
                return Err(format!("Invalid resolver object: {name}"));
            }
            // `ParseSeverity` (`:256-273`) and `ParseAction` (`:275-292`). The severity only chose
            // Reforged's log level; it is checked, and the action is what is kept.
            let severity = text_of(value, "log_level", "error").to_ascii_lowercase();
            let action = text_of(value, "on_fail", "halt").to_ascii_lowercase();
            let may_continue = match action.as_str() {
                "continue" => true,
                "halt" | "abort" | "fail" => false,
                _ => return Err(format!("Invalid resolver failure policy: {name}")),
            };
            if !matches!(severity.as_str(), "warning" | "warn" | "error") {
                return Err(format!("Invalid resolver failure policy: {name}"));
            }

            let steps_of = |holder: &Value| -> Result<Vec<Step>, String> {
                let steps = holder
                    .get("steps")
                    .and_then(Value::as_array)
                    .filter(|steps| !steps.is_empty())
                    .ok_or_else(|| format!("Resolver has no steps: {name}"))?;
                steps
                    .iter()
                    .map(|step| parse_step(namespace, &name, step))
                    .collect()
            };
            let attempts = match value.get("attempts") {
                Some(attempts) => {
                    let attempts = attempts
                        .as_array()
                        .filter(|attempts| !attempts.is_empty())
                        .ok_or_else(|| format!("Resolver has invalid attempts: {name}"))?;
                    attempts
                        .iter()
                        .map(|attempt| {
                            Ok(Attempt {
                                name: text_of(attempt, "name", "attempt").to_string(),
                                steps: steps_of(attempt)?,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?
                }
                None => vec![Attempt {
                    name: "default".to_string(),
                    steps: steps_of(value)?,
                }],
            };
            self.resolvers.insert(
                name,
                Resolver {
                    attempts,
                    may_continue,
                },
            );
        }
        Ok(())
    }

    /// Every resolver's qualified name, sorted.
    pub fn resolver_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.resolvers.keys().cloned().collect();
        names.sort();
        names
    }

    /// Follow a resolver to its address.
    ///
    /// Port of `Patterns::ResolvePointer` (`patterns.cpp:712-1031`): each attempt runs its steps in
    /// order, a step that fails ends its attempt, the first attempt that fills `final` is the
    /// answer, and when every attempt fails the last one's reason is reported.
    pub fn resolve(&self, name: &str, scanner: &Scanner, memory: &dyn Memory) -> Resolution {
        let Some(resolver) = self.resolvers.get(name) else {
            return Resolution {
                value: None,
                message: "Resolver definition not found".to_string(),
                may_continue: false,
                trace: Vec::new(),
            };
        };
        let mut last = Resolution {
            value: None,
            message: String::new(),
            may_continue: resolver.may_continue,
            trace: Vec::new(),
        };
        for attempt in &resolver.attempts {
            let result = self.run_attempt(attempt, scanner, memory, resolver.may_continue);
            if result.value.is_some() {
                return result;
            }
            last = result;
        }
        if resolver.attempts.len() > 1 && !last.message.is_empty() {
            last.message = format!("all attempts failed; last_error={}", last.message);
        }
        last
    }

    fn run_attempt(
        &self,
        attempt: &Attempt,
        scanner: &Scanner,
        memory: &dyn Memory,
        may_continue: bool,
    ) -> Resolution {
        let mut values: HashMap<&str, u32> = HashMap::new();
        let mut trace = Vec::new();
        for step in &attempt.steps {
            let mut input = 0u32;
            let outcome = self.run_step(step, &values, scanner, memory, &mut input);
            let label = if step.name.is_empty() {
                &step.output
            } else {
                &step.name
            };
            match outcome {
                Ok(output) => {
                    trace.push(format!(
                        "{label} {} {input:#x} -> {output:#x}",
                        step.op.name()
                    ));
                    values.insert(&step.output, output);
                }
                Err(detail) => {
                    trace.push(format!("{label} {} {input:#x} failed", step.op.name()));
                    return Resolution {
                        value: None,
                        message: format!("{}: step '{label}': {detail}", attempt.name),
                        may_continue,
                        trace,
                    };
                }
            }
        }
        match values.get("final") {
            Some(&value) => Resolution {
                value: Some(value),
                message: String::new(),
                may_continue,
                trace,
            },
            None => Resolution {
                value: None,
                message: format!(
                    "{}: resolver did not populate final output",
                    attempt.name
                ),
                may_continue,
                trace,
            },
        }
    }

    /// One step: its output, or why it has none. The switch of `ResolvePointer`,
    /// `patterns.cpp:788-993`, case by case.
    fn run_step(
        &self,
        step: &Step,
        values: &HashMap<&str, u32>,
        scanner: &Scanner,
        memory: &dyn Memory,
        input: &mut u32,
    ) -> Result<u32, String> {
        let pattern = |name: &str| {
            self.patterns
                .get(name)
                .ok_or_else(|| format!("pattern not found: {name}"))
        };
        let reference = |name: &str, what: &str| {
            values
                .get(name)
                .copied()
                .ok_or_else(|| format!("{what} reference not found: {name}"))
        };
        let mask = |pattern: &'_ Pattern| (!pattern.mask.is_empty()).then(|| pattern.mask.clone());
        let nonzero = |output: u32, detail: &str| {
            if output != 0 {
                Ok(output)
            } else {
                Err(detail.to_string())
            }
        };

        match step.op {
            Op::Scan => {
                let pattern = pattern(&step.pattern)?;
                let output = if !pattern.assertion_file.is_empty()
                    || !pattern.assertion_message.is_empty()
                {
                    scanner.find_assertion(
                        &pattern.assertion_file,
                        &pattern.assertion_message,
                        pattern.line_number as u32,
                        pattern.offset,
                    )
                } else {
                    scanner.find(
                        &pattern.bytes,
                        mask(pattern).as_deref().map(str::as_bytes),
                        pattern.offset,
                        pattern.section,
                    )
                };
                nonzero(output, "scan returned null")
            }
            Op::ScanInRange => {
                let pattern = pattern(&step.pattern)?;
                let start = reference(&step.start, "start")?.wrapping_add(step.start_add as u32);
                let end = reference(&step.end, "end")?.wrapping_add(step.end_add as u32);
                *input = start;
                let output = scanner.find_in_range(
                    &pattern.bytes,
                    mask(pattern).as_deref().map(str::as_bytes),
                    pattern.offset,
                    start,
                    end,
                );
                nonzero(output, "range scan returned null")
            }
            Op::ToFunctionStart => {
                *input = reference(&step.input, "input")?;
                nonzero(
                    scanner.to_function_start(*input, step.scan_range as u32),
                    "resolved function start is null",
                )
            }
            Op::FunctionFromNearCall => {
                *input = reference(&step.input, "input")?;
                nonzero(
                    scanner.function_from_near_call(*input, step.check_valid_ptr),
                    "near-call target is null",
                )
            }
            Op::FindUseOfString => {
                let (literal, offset, section) = if step.pattern.is_empty() {
                    (step.literal.as_bytes().to_vec(), 0, Section::Text)
                } else {
                    let pattern = pattern(&step.pattern)?;
                    (pattern.bytes.clone(), pattern.offset, pattern.section)
                };
                if literal.is_empty() {
                    return Err("string literal is empty".to_string());
                }
                let nth = step.nth as usize;
                let output = if step.wide {
                    let wide: Vec<u16> = literal.iter().map(|&byte| u16::from(byte)).collect();
                    scanner.find_nth_use_of_wide_string(&wide, nth, offset, section)
                } else {
                    scanner.find_nth_use_of_string(&literal, nth, offset, section)
                };
                nonzero(output, "string use scan returned null")
            }
            Op::Dereference => {
                *input = reference(&step.input, "input")?;
                if *input == 0 {
                    return Err("cannot dereference null input".to_string());
                }
                let output = memory
                    .read_u32(*input)
                    .ok_or("cannot dereference unreadable input")?;
                nonzero(output, "dereferenced pointer is null")
            }
            Op::ReadU32 => {
                *input = reference(&step.input, "input")?;
                if *input == 0 {
                    return Err("cannot read from null input".to_string());
                }
                memory
                    .read_u32(*input)
                    .ok_or_else(|| "cannot read from unreadable input".to_string())
            }
            Op::Add => {
                *input = reference(&step.input, "input")?;
                nonzero(
                    input.wrapping_add(step.value as u32),
                    "arithmetic result is null",
                )
            }
            Op::Divide => {
                *input = reference(&step.input, "input")?;
                if step.value == 0 {
                    return Err("cannot divide by zero".to_string());
                }
                Ok(*input / step.value as u32)
            }
            Op::ValidateSection => {
                *input = reference(&step.input, "input")?;
                let section = step.section.unwrap_or(Section::Text);
                if scanner.is_valid_ptr(*input, section) {
                    Ok(*input)
                } else {
                    Err("pointer is outside expected section".to_string())
                }
            }
        }
    }
}

/// One step of a document (`ParseResolverStep`, `patterns.cpp:397-490`): the same fields, defaults
/// and requirements per kind of step.
fn parse_step(namespace: &str, resolver: &str, value: &Value) -> Result<Step, String> {
    if !value.is_object() {
        return Err(format!("Invalid resolver step in {resolver}"));
    }
    let op = Op::parse(text_of(value, "op", ""))
        .ok_or_else(|| format!("Invalid resolver step op in {resolver}"))?;
    let number = |key: &str, default: i32| -> Result<i32, String> {
        match value.get(key) {
            None => Ok(default),
            Some(field) => int_value(field)
                .ok_or_else(|| format!("Invalid numeric resolver step field in {resolver}")),
        }
    };
    let flag = |key: &str, default: bool| -> Result<bool, String> {
        match value.get(key) {
            None => Ok(default),
            Some(field) => {
                bool_value(field).ok_or_else(|| format!("Invalid {key} field in {resolver}"))
            }
        }
    };
    let raw_pattern = text_of(value, "pattern", "");
    let output = text_of(value, "out", "");
    let step = Step {
        name: text_of(value, "name", text_of(value, "op", "")).to_string(),
        op,
        input: text_of(value, "in", "").to_string(),
        output: if output.is_empty() { "value" } else { output }.to_string(),
        pattern: if raw_pattern.is_empty() {
            String::new()
        } else {
            qualify(namespace, raw_pattern)
        },
        literal: text_of(value, "literal", "").to_string(),
        start: text_of(value, "start", "").to_string(),
        end: text_of(value, "end", "").to_string(),
        start_add: number("start_add", 0)?,
        end_add: number("end_add", 0)?,
        value: number("value", 0)?,
        nth: number("nth", 0)?,
        scan_range: number("scan_range", 0xFF)?,
        check_valid_ptr: flag("check_valid_ptr", true)?,
        wide: flag("wide", false)?,
        section: value
            .get("section")
            .map(|field| section(field.as_str().unwrap_or("text"))),
    };

    let needs_pattern = matches!(step.op, Op::Scan | Op::ScanInRange);
    let needs_input = matches!(
        step.op,
        Op::ToFunctionStart
            | Op::FunctionFromNearCall
            | Op::Dereference
            | Op::ReadU32
            | Op::Add
            | Op::Divide
            | Op::ValidateSection
    );
    if needs_pattern && step.pattern.is_empty() {
        return Err(format!(
            "Resolver step missing pattern reference in {resolver}"
        ));
    }
    if needs_input && step.input.is_empty() {
        return Err(format!(
            "Resolver step missing input reference in {resolver}"
        ));
    }
    if step.op == Op::FindUseOfString && step.pattern.is_empty() && step.literal.is_empty() {
        return Err(format!(
            "Resolver find_use_of_string step missing pattern/literal in {resolver}"
        ));
    }
    if step.op == Op::ScanInRange && (step.start.is_empty() || step.end.is_empty()) {
        return Err(format!(
            "Resolver range scan missing start/end reference in {resolver}"
        ));
    }
    if step.op == Op::ValidateSection && step.section.is_none() {
        return Err(format!(
            "Resolver validate_section missing section in {resolver}"
        ));
    }
    Ok(step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::tests::{BASE, RDATA_RVA, TEXT_RVA, image};
    use serde_json::json;

    struct Table(HashMap<u32, u32>);

    impl Memory for Table {
        fn read_u32(&self, address: u32) -> Option<u32> {
            self.0.get(&address).copied()
        }
    }

    fn nothing() -> Table {
        Table(HashMap::new())
    }

    /// +0: push ebp; mov ebp, esp; +3: call -> +0x10; +8: mov eax, [rdata+8]; +0x10: push ebp ...
    fn scanner() -> Scanner {
        let mut text = vec![0x90u8; 0x40];
        text[0..3].copy_from_slice(&[0x55, 0x8B, 0xEC]);
        text[3] = 0xE8;
        text[4..8].copy_from_slice(&8u32.to_le_bytes());
        text[8] = 0xA1;
        text[9..13].copy_from_slice(&(BASE + RDATA_RVA + 8).to_le_bytes());
        text[0x10..0x13].copy_from_slice(&[0x55, 0x8B, 0xEC]);
        text[0x20..0x27].copy_from_slice(&[0x5F, 0x5E, 0x5B, 0x8B, 0xE5, 0x5D, 0xC3]);
        Scanner::from_file(&image(&text, &[0; 0x20]), BASE).unwrap()
    }

    fn document() -> Value {
        json!({
            "namespace": "demo",
            "patterns": {
                "call_site": {
                    "pattern": "\\x55\\x8B\\xEC\\xE8\\x00\\x00\\x00\\x00\\xA1",
                    "mask": "xxxx????x",
                    "offset": "0x3",
                    "section": "text"
                },
                "epilogue": { "pattern": "\\x5F\\x5E\\x5B\\x8B\\xE5\\x5D\\xC3", "mask": "xxxxxxx" },
                "global_load": { "pattern": "\\xA1", "mask": "x", "offset": 1 }
            },
            "resolvers": {
                "site": { "steps": [ { "op": "scan", "pattern": "call_site", "out": "final" } ] },
                "callee": { "steps": [
                    { "op": "scan", "pattern": "call_site", "out": "site" },
                    { "op": "function_from_near_call", "in": "site", "out": "final" }
                ] },
                "return_point": { "steps": [
                    { "op": "scan", "pattern": "call_site", "out": "site" },
                    { "op": "scan_in_range", "pattern": "epilogue", "start": "site", "end": "site",
                      "end_add": "0x80", "out": "final" }
                ] },
                "global": { "steps": [
                    { "op": "scan", "pattern": "global_load", "out": "operand" },
                    { "op": "dereference", "in": "operand", "out": "pointer" },
                    { "op": "validate_section", "in": "pointer", "section": "rdata", "out": "checked" },
                    { "op": "add", "in": "checked", "value": 4, "out": "final" }
                ] },
                "enclosing": { "steps": [
                    { "op": "scan", "pattern": "global_load", "out": "inside" },
                    { "op": "to_function_start", "in": "inside", "scan_range": 64, "out": "final" }
                ] },
                "fallback": { "on_fail": "continue", "attempts": [
                    { "name": "new build", "steps": [ { "op": "scan", "pattern": "missing", "out": "final" } ] },
                    { "name": "old build", "steps": [ { "op": "scan", "pattern": "epilogue", "out": "final" } ] }
                ] },
                "absent": { "on_fail": "continue", "log_level": "warning", "steps": [
                    { "op": "scan", "pattern": "epilogue", "out": "found" },
                    { "op": "divide", "in": "found", "value": 0, "out": "final" }
                ] }
            }
        })
    }

    fn loaded() -> Patterns {
        let mut patterns = Patterns::new();
        patterns.load(&document()).unwrap();
        patterns
    }

    #[test]
    fn a_resolver_follows_its_steps_to_an_address() {
        let (patterns, scanner) = (loaded(), scanner());
        let text = BASE + TEXT_RVA;
        let value = |name: &str| patterns.resolve(name, &scanner, &nothing()).value;
        assert_eq!(value("demo.site"), Some(text + 3));
        assert_eq!(value("demo.callee"), Some(text + 0x10));
        assert_eq!(value("demo.return_point"), Some(text + 0x20));
        assert_eq!(value("demo.enclosing"), Some(text));
    }

    #[test]
    fn a_pointer_is_followed_through_the_process_memory() {
        let (patterns, scanner) = (loaded(), scanner());
        let operand = BASE + TEXT_RVA + 9;
        let memory = Table(HashMap::from([(operand, BASE + RDATA_RVA + 8)]));
        assert_eq!(
            patterns.resolve("demo.global", &scanner, &memory).value,
            Some(BASE + RDATA_RVA + 12)
        );
        let unreadable = patterns.resolve("demo.global", &scanner, &nothing());
        assert_eq!(unreadable.value, None);
        assert!(unreadable.message.contains("cannot dereference unreadable input"));
        assert!(!unreadable.may_continue);
    }

    #[test]
    fn the_first_attempt_that_works_is_the_answer() {
        let (patterns, scanner) = (loaded(), scanner());
        let result = patterns.resolve("demo.fallback", &scanner, &nothing());
        assert_eq!(result.value, Some(BASE + TEXT_RVA + 0x20));
    }

    #[test]
    fn a_failure_names_its_step_and_says_whether_to_carry_on() {
        let (patterns, scanner) = (loaded(), scanner());
        let result = patterns.resolve("demo.absent", &scanner, &nothing());
        assert_eq!(result.value, None);
        assert!(result.message.contains("cannot divide by zero"), "{}", result.message);
        assert!(result.may_continue);
        let unknown = patterns.resolve("demo.nothing", &scanner, &nothing());
        assert_eq!(unknown.message, "Resolver definition not found");
    }

    #[test]
    fn a_document_that_breaks_the_format_is_refused() {
        let refused = |document: Value| Patterns::new().load(&document).unwrap_err();
        assert!(
            refused(json!({ "resolvers": { "a": { "steps": [ { "op": "teleport" } ] } } }))
                .contains("Invalid resolver step op")
        );
        assert!(
            refused(json!({ "resolvers": { "a": { "steps": [ { "op": "scan" } ] } } }))
                .contains("missing pattern reference")
        );
        assert!(
            refused(json!({ "resolvers": { "a": { "steps": [] } } })).contains("has no steps")
        );
        assert!(
            refused(json!({ "patterns": { "a": { "pattern": "\\xZZ" } } }))
                .contains("Invalid pattern entry")
        );
        assert!(refused(json!({ "patterns": { "a": {} } })).contains("no usable scanner inputs"));
        let mut twice = Patterns::new();
        twice.load(&document()).unwrap();
        assert!(twice.load(&document()).unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn numbers_are_read_as_reforged_reads_them() {
        assert_eq!(parse_int("0x20"), Some(32));
        assert_eq!(parse_int("-0x6"), Some(-6));
        assert_eq!(parse_int("17"), Some(17));
        assert_eq!(parse_int("010"), Some(8));
        assert_eq!(parse_int("0"), Some(0));
        assert_eq!(parse_int("12 "), None);
        assert_eq!(decode_literal("\\x75\\x04A\\0"), Some(vec![0x75, 0x04, b'A', 0]));
    }
}
