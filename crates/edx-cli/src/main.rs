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
  edx fmt [-o OUT] FILE                    write a document in canonical form
  edx hash [--format text|json] FILE       print the hashes of its design, rules and constraints
  edx constraints [--format text|json] FILE
                                           list the constraints in force, derived and authored
  edx schema                               print the JSON Schema this tool checks against
  edx explain CODE                         describe a diagnostic code, such as EDX-E2001
  edx codes                                list every diagnostic code
  edx from-kicad [-o OUT] [--project PRO] [--fixed edge|locked|all|none] [--no-layout] BOARD
                                           convert a KiCad 6+ board (.kicad_pcb) to EDX; the
                                           project file beside it gives the net classes

from-kicad fixes the parts the author locked, mounting holes and parts at the board's edge
(--fixed edge), and writes the author's placement and copper as `layout` unless --no-layout.
What it left out or approximated goes to standard error.

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
    let mut project = None;
    let mut kicad = edx_kicad::Options::default();
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
            "--project" => project = Some(it.next().ok_or("--project needs a file name")?.clone()),
            "--no-layout" => kicad.layout = false,
            "--fixed" => {
                kicad.fixed = match it.next().map(String::as_str) {
                    Some("edge") => edx_kicad::Fixed::Edge,
                    Some("locked") => edx_kicad::Fixed::Locked,
                    Some("all") => edx_kicad::Fixed::All,
                    Some("none") => edx_kicad::Fixed::None,
                    other => return Err(format!("--fixed is edge, locked, all or none, not {other:?}")),
                }
            }
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
        "fmt" if files.len() == 1 => fmt(&files[0], output.as_deref()),
        "hash" if files.len() == 1 => hash(&files[0], format),
        "constraints" if files.len() == 1 => constraints(&files[0], format),
        "from-kicad" if files.len() == 1 => from_kicad(&files[0], project.as_deref(), &kicad, output.as_deref()),
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
        "check" | "merge" | "explain" | "fmt" | "hash" | "from-kicad" => {
            Err(format!("wrong number of arguments for `{command}`"))
        }
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

/// Reads a document for canonical form, which needs every member known (section 13).
fn read_known(file: &str) -> Result<Result<edx_format::Document, Checked>, String> {
    let checked = edx_format::check(&read(file)?);
    let newer = checked.diagnostics.iter().any(|d| d.code == "EDX-W1006");
    Ok(match checked.document {
        Some(ref d) if checked.is_ok() && !newer => Ok(d.clone()),
        _ => Err(checked),
    })
}

fn refuse(file: &str, checked: &Checked) -> ExitCode {
    print_text(file, checked);
    if checked.is_ok() {
        eprintln!("{file}: a document of a newer minor version has no canonical form this tool can write");
    }
    ExitCode::from(1)
}

fn fmt(file: &str, output: Option<&str>) -> Result<ExitCode, String> {
    let document = match read_known(file)? {
        Ok(d) => d,
        Err(checked) => return Ok(refuse(file, &checked)),
    };
    let text = edx_format::to_canonical_string(&document);
    match output {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?,
        None => std::io::stdout().write_all(text.as_bytes()).map_err(|e| e.to_string())?,
    }
    Ok(ExitCode::SUCCESS)
}

fn hash(file: &str, format: Format) -> Result<ExitCode, String> {
    let document = match read_known(file)? {
        Ok(d) => d,
        Err(checked) => return Ok(refuse(file, &checked)),
    };
    let hashes = edx_format::hashes(&document);
    match format {
        Format::Json => println!("{}", serde_json::to_string(&hashes).map_err(|e| e.to_string())?),
        Format::Text => {
            for (section, hash) in
                [("design", &hashes.design), ("rules", &hashes.rules), ("constraints", &hashes.constraints)]
            {
                if let Some(hash) = hash {
                    println!("{section:<12} {hash}");
                }
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn constraints(file: &str, format: Format) -> Result<ExitCode, String> {
    let document = match read_known(file)? {
        Ok(d) => d,
        Err(checked) => return Ok(refuse(file, &checked)),
    };
    let constraints = edx_format::derive::constraints(&document);
    match format {
        Format::Json => println!("{}", serde_json::to_string(&constraints).map_err(|e| e.to_string())?),
        Format::Text => {
            for c in &constraints {
                let origin = if c.origin == edx_format::model::Origin::Derived { "derived" } else { "authored" };
                println!("{}  ({origin})", c.id);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
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

fn from_kicad(
    board: &str,
    project: Option<&str>,
    options: &edx_kicad::Options,
    output: Option<&str>,
) -> Result<ExitCode, String> {
    let text = String::from_utf8(read(board)?).map_err(|_| format!("{board}: not UTF-8"))?;
    // The project file beside the board, unless one is named.
    let beside = std::path::Path::new(board).with_extension("kicad_pro");
    let project = match project {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?),
        None if board != "-" => std::fs::read_to_string(&beside).ok(),
        None => None,
    };
    let converted = edx_kicad::convert(&text, project.as_deref(), options).map_err(|e| format!("{board}: {e}"))?;
    let written = edx_format::to_string(&converted.document);
    for note in &converted.notes {
        eprintln!("{board}: note: {note}");
    }
    // What was written is checked as any reader would check it.
    let checked = edx_format::check(written.as_bytes());
    let label = output.unwrap_or("(output)");
    for d in &checked.diagnostics {
        eprintln!("{label}: {d}");
    }
    if let Some(doc) = &checked.document {
        for d in edx_format::incomplete(doc) {
            eprintln!("{label}: {d}");
        }
    }
    match output {
        Some(path) => std::fs::write(path, &written).map_err(|e| format!("{path}: {e}"))?,
        None => std::io::stdout().write_all(written.as_bytes()).map_err(|e| e.to_string())?,
    }
    Ok(status(checked.is_ok()))
}
