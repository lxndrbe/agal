//! Hybrid per-node notes: auto header regenerated; human body preserved.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::findings::Finding;
use crate::{Audiolabs, Edge, Node};

const AUTO_START: &str = "<!-- AUDIOLABS:AUTO-START -->";
const AUTO_END: &str = "<!-- AUDIOLABS:AUTO-END -->";
const HUMAN_MARK: &str = "<!-- AUDIOLABS:HUMAN — edit below this line; preserved on regenerate -->";

/// Write `notes/<name>.md` for each plugin and crate node.
pub fn write_notes(output_dir: &Path, graph: &Audiolabs) -> Result<usize, String> {
    let notes_dir = output_dir.join("notes");
    fs::create_dir_all(&notes_dir)
        .map_err(|e| format!("cannot create {}: {}", notes_dir.display(), e))?;

    let findings_by_node = index_findings(&graph.findings);
    let mut written = 0usize;

    for n in &graph.nodes {
        if n.kind != "plugin" && n.kind != "crate" {
            continue;
        }
        let path = notes_dir.join(format!("{}.md", sanitize_filename(&n.name)));
        let human = load_human_body(&path);
        let auto = render_auto_section(n, graph, &findings_by_node);
        let body = compose(&auto, human.as_deref());
        fs::write(&path, body).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
        written += 1;
    }

    write_index(&notes_dir, graph)?;
    Ok(written)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn index_findings(findings: &[Finding]) -> BTreeMap<String, Vec<&Finding>> {
    let mut m: BTreeMap<String, Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        if let Some(node) = &f.node {
            m.entry(node.clone()).or_default().push(f);
        }
    }
    m
}

fn load_human_body(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    if let Some(idx) = content.find(HUMAN_MARK) {
        let after = &content[idx + HUMAN_MARK.len()..];
        return Some(after.trim_start_matches(['\r', '\n']).to_string());
    }
    // Legacy / first human draft without marker: keep everything after AUTO_END if present.
    if let Some(idx) = content.find(AUTO_END) {
        let after = &content[idx + AUTO_END.len()..];
        let trimmed = after
            .trim_start_matches(['\r', '\n'])
            .trim_start_matches(HUMAN_MARK)
            .trim_start_matches(['\r', '\n']);
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn compose(auto: &str, human: Option<&str>) -> String {
    let mut s = String::with_capacity(auto.len() + 512);
    s.push_str(AUTO_START);
    s.push('\n');
    s.push_str(auto);
    if !auto.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(AUTO_END);
    s.push_str("\n\n");
    s.push_str(HUMAN_MARK);
    s.push_str("\n\n");
    match human {
        Some(h) if !h.is_empty() => {
            s.push_str(h);
            if !h.ends_with('\n') {
                s.push('\n');
            }
        }
        _ => {
            s.push_str("## Intent\n\n_Why this crate/plugin exists. Edit freely._\n\n");
            s.push_str("## Open\n\n- [ ] \n\n");
            s.push_str("## Decisions\n\n_Architecture choices worth remembering._\n");
        }
    }
    s
}

fn render_auto_section(
    n: &Node,
    graph: &Audiolabs,
    findings_by_node: &BTreeMap<String, Vec<&Finding>>,
) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {}\n", n.name);
    let _ = writeln!(
        s,
        "> Auto-generated from workspace scan. Do not edit between AUTO markers.\n"
    );
    let _ = writeln!(s, "| | |");
    let _ = writeln!(s, "|---|---|");
    let _ = writeln!(s, "| kind | `{}` |", n.kind);
    let _ = writeln!(s, "| path | `{}` |", n.id);
    if let Some(v) = &n.version {
        let _ = writeln!(s, "| version | {} |", v);
    }
    if let Some(d) = &n.description {
        let _ = writeln!(s, "| description | {} |", d);
    }
    if !n.frameworks.is_empty() {
        let _ = writeln!(s, "| frameworks | {} |", n.frameworks.join(", "));
    }
    if let Some(m) = &n.migration_status {
        let _ = writeln!(s, "| migration | **{}** |", m);
    }
    let _ = writeln!(s, "| generated | `{}` |", graph.generated_at);
    let _ = writeln!(s);

    if !n.internal_deps.is_empty() {
        let _ = writeln!(s, "## deps (workspace)");
        for d in &n.internal_deps {
            let _ = writeln!(s, "- `{}`", short_id(d));
        }
        let _ = writeln!(s);
    }

    // Dependents: who points at this node (blast radius)
    let dependents: Vec<&Edge> = graph.edges.iter().filter(|e| e.to == n.id).collect();
    if !dependents.is_empty() {
        let _ = writeln!(s, "## dependents (inbound)");
        for e in dependents {
            let _ = writeln!(
                s,
                "- `{}` --{}--> `{}`",
                short_id(&e.from),
                e.kind,
                short_id(&e.to)
            );
        }
        let _ = writeln!(s);
    }

    // Outbound semantic edges
    let outbound: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|e| e.from == n.id && e.kind != "depends_on" && e.kind != "build_depends_on")
        .collect();
    if !outbound.is_empty() {
        let _ = writeln!(s, "## semantic edges");
        for e in outbound {
            let note = e
                .note
                .as_ref()
                .map(|n| format!(" — {}", n))
                .unwrap_or_default();
            let _ = writeln!(s, "- **{}** → `{}`{}", e.kind, short_id(&e.to), note);
        }
        let _ = writeln!(s);
    }

    if let Some(ast) = &n.ast_summary {
        let _ = writeln!(s, "## structure");
        if !ast.plugin_logic_impls.is_empty() {
            let _ = writeln!(
                s,
                "- logic: {}",
                ast.plugin_logic_impls
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !ast.params_structs.is_empty() {
            for ps in &ast.params_structs {
                let nfields = ast.params_fields.get(ps).map(|f| f.len()).unwrap_or(0);
                let _ = writeln!(s, "- params: {} ({} fields)", ps, nfields);
            }
        }
        if !ast.process_hooks.is_empty() {
            let _ = writeln!(
                s,
                "- process: {}",
                ast.process_hooks
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !ast.editor_functions.is_empty() {
            let _ = writeln!(s, "- editor: yes");
        }
        if !ast.plugin_formats.is_empty() {
            let _ = writeln!(
                s,
                "- formats: {}",
                ast.plugin_formats
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !ast.ipc_signals.is_empty() {
            let _ = writeln!(
                s,
                "- ipc: {}",
                ast.ipc_signals
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !ast.slint_components.is_empty() {
            let _ = writeln!(s, "- slint components: {}", ast.slint_components.len());
        }
        if !ast.slint_exports.is_empty() {
            let _ = writeln!(s, "- slint exports: {}", ast.slint_exports.len());
        }
        if ast.process_method_count > 0 && n.kind == "crate" {
            let _ = writeln!(s, "- process methods (DSP): {}", ast.process_method_count);
        }
        if !ast.public_api.is_empty() && n.kind == "crate" {
            let _ = writeln!(
                s,
                "- public_api symbols: {} (see json)",
                ast.public_api.len()
            );
        }
        let roles = ast.role_tags();
        if !roles.is_empty() {
            let _ = writeln!(
                s,
                "- roles: {}",
                roles.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
        let _ = writeln!(s);
    }

    if let Some(fs) = findings_by_node.get(&n.id) {
        let _ = writeln!(s, "## findings");
        for f in fs {
            let mut line = format!("- [{}] **{}**: {}", f.severity, f.code, f.message);
            if let Some(path) = &f.path {
                line.push_str(&format!(" · `{}`", path));
            }
            if let Some(fix) = &f.fix {
                line.push_str(&format!(" · fix: {}", fix));
            }
            let _ = writeln!(s, "{}", line);
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(
        s,
        "## agent focus\n\
         Read this note after `audiolabs.agent.md`.  \n\
         Escalate: `{}` in json / `agal --plugin {} .`\n",
        n.id, n.name
    );

    s
}

fn write_index(notes_dir: &Path, graph: &Audiolabs) -> Result<(), String> {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# notes index\n\n\
         Auto-generated. Per-node notes for focus work.\n\n\
         ## plugins\n"
    );
    for n in graph.nodes.iter().filter(|n| n.kind == "plugin") {
        let _ = writeln!(
            s,
            "- [{}]({}.md) — `{}`{}",
            n.name,
            sanitize_filename(&n.name),
            n.id,
            n.migration_status
                .as_ref()
                .map(|m| format!(" · {}", m))
                .unwrap_or_default()
        );
    }
    let _ = writeln!(s, "\n## crates\n");
    for n in graph.nodes.iter().filter(|n| n.kind == "crate") {
        let _ = writeln!(
            s,
            "- [{}]({}.md) — `{}`",
            n.name,
            sanitize_filename(&n.name),
            n.id
        );
    }
    let path = notes_dir.join("_index.md");
    fs::write(&path, s).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    Ok(())
}

fn short_id(id: &str) -> String {
    id.trim_start_matches("plugins/")
        .trim_start_matches("crates/")
        .to_string()
}
