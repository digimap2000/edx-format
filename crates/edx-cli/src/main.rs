//! `edx`: check, merge and explain EDX documents.

use std::io::Write;
use std::process::ExitCode;

use edx_format::{Checked, Diagnostic, CODES};
use serde::Serialize;

const USAGE: &str = "\
edx: check and merge EDX documents (Electronics Design eXchange)

Usage:
  edx check [--format text|json] FILE...   check documents; exit status 1 if any has errors
  edx merge [--format text|json] [-o OUT] FIRST SECOND
                                           merge two documents and check the result
  edx schema                               print the JSON Schema this tool checks against
  edx explain CODE                         describe a diagnostic code, such as EDX-E2001
  edx codes                                list every diagnostic code

FILE may be - for standard input. With --format json, check and merge print one JSON
object per run: {\"documents\": [{\"file\", \"ok\", \"diagnostics\"}]}, and merge adds
\"result\" (the merged document) when it succeeds.";

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Text,
    Json,
}

#[derive(Serialize)]
struct Outcome<'a> {
    file: String,
    ok: bool,
    diagnostics: &'a [Diagnostic],
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("edx: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some((command, rest)) = args.split_first() else { return Err("no command given".into()) };
    let mut format = Format::Text;
    let mut output = None;
    let mut files = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--format" => {
                format = match it.next().map(String::as_str) {
                    Some("text") => Format::Text,
                    Some("json") => Format::Json,
                    other => return Err(format!("unknown format {other:?}")),
                }
            }
            "-o" | "--output" => output = Some(it.next().ok_or("-o needs a file name")?.clone()),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(ExitCode::SUCCESS);
            }
            _ => files.push(arg.clone()),
        }
    }
    match command.as_str() {
        "check" if !files.is_empty() => check(&files, format),
        "merge" if files.len() == 2 => merge(&files[0], &files[1], format, output.as_deref()),
        "schema" => {
            print!("{}", edx_format::SCHEMA);
            Ok(ExitCode::SUCCESS)
        }
        "explain" if files.len() == 1 => explain(&files[0]),
        "codes" => {
            for (code, summary) in CODES {
                println!("{code}  {summary}");
            }
            Ok(ExitCode::SUCCESS)
        }
        "help" | "-h" | "--help" => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "check" | "merge" | "explain" => Err(format!("wrong number of arguments for `{command}`")),
        _ => Err(format!("unknown command `{command}`")),
    }
}

fn read(file: &str) -> Result<Vec<u8>, String> {
    if file == "-" {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes).map_err(|e| format!("standard input: {e}"))?;
        Ok(bytes)
    } else {
        std::fs::read(file).map_err(|e| format!("{file}: {e}"))
    }
}

fn status(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_text(file: &str, checked: &Checked) {
    for d in &checked.diagnostics {
        println!("{file}: {d}");
    }
    let (errors, warnings) = (checked.errors().count(), checked.warnings().count());
    if errors == 0 {
        eprintln!("{file}: ok ({warnings} warnings)");
    } else {
        eprintln!("{file}: {errors} errors, {warnings} warnings");
    }
}

fn check(files: &[String], format: Format) -> Result<ExitCode, String> {
    let mut all_ok = true;
    let mut results = Vec::new();
    for file in files {
        let checked = edx_format::check(&read(file)?);
        all_ok &= checked.is_ok();
        results.push((file.clone(), checked));
    }
    match format {
        Format::Text => results.iter().for_each(|(file, checked)| print_text(file, checked)),
        Format::Json => {
            let documents: Vec<Outcome> = results
                .iter()
                .map(|(file, c)| Outcome { file: file.clone(), ok: c.is_ok(), diagnostics: &c.diagnostics })
                .collect();
            println!("{}", serde_json::json!({ "documents": documents }));
        }
    }
    Ok(status(all_ok))
}

fn merge(first: &str, second: &str, format: Format, output: Option<&str>) -> Result<ExitCode, String> {
    let parse = |file: &str| -> Result<Result<serde_json::Value, Checked>, String> {
        let checked = edx_format::check(&read(file)?);
        Ok(match checked.value {
            Some(ref v) if checked.is_ok() => Ok(v.clone()),
            _ => Err(checked),
        })
    };
    let (a, b) = (parse(first)?, parse(second)?);
    let (label, merged) = match (a, b) {
        (Ok(a), Ok(b)) => (format!("{first} + {second}"), edx_format::merge(&a, &b)),
        (Err(c), _) => (first.to_owned(), c),
        (_, Err(c)) => (second.to_owned(), c),
    };
    let result = merged.value.as_ref().filter(|_| merged.is_ok());
    if let (Some(path), Some(value)) = (output, result) {
        std::fs::write(path, edx_format::to_string_value(value)).map_err(|e| format!("{path}: {e}"))?;
    }
    match format {
        Format::Text => {
            print_text(&label, &merged);
            if let (None, Some(value)) = (output, result) {
                std::io::stdout()
                    .write_all(edx_format::to_string_value(value).as_bytes())
                    .map_err(|e| e.to_string())?;
            }
        }
        Format::Json => {
            let mut report = serde_json::json!({
                "documents": [Outcome { file: label, ok: merged.is_ok(), diagnostics: &merged.diagnostics }],
            });
            if let Some(value) = result {
                report["result"] = value.clone();
            }
            println!("{report}");
        }
    }
    Ok(status(merged.is_ok()))
}

fn explain(code: &str) -> Result<ExitCode, String> {
    let wanted = code.to_ascii_uppercase();
    let wanted = if wanted.starts_with("EDX-") { wanted } else { format!("EDX-{wanted}") };
    match CODES.iter().find(|(c, _)| *c == wanted) {
        Some((c, summary)) => {
            println!("{c}: {summary}");
            Ok(ExitCode::SUCCESS)
        }
        None => Err(format!("no code {code}")),
    }
}
