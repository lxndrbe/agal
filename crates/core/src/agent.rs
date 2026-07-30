use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::delta;
use crate::findings::{Finding, Severity, actionable, count_by_severity, health};
use crate::{Audiolabs, Edge, Node};

/// Compact markdown for AI agents (~1–3k tokens for typical audio workspace).
///
/// `skills_dir` = `<output>/skills` when present — indexes workspace skills + authoring rules.
pub fn render_agent_md(graph: &Audiolabs, skills_dir: Option<&Path>) -> String {
    let mut s = String::with_capacity(4096);
    let counts = count_by_severity(&graph.findings);
    let h = health(&graph.findings);
    let err_n = counts.get("error").copied().unwrap_or(0);
    let warn_n = counts.get("warn").copied().unwrap_or(0);
    let info_n = counts.get("info").copied().unwrap_or(0);
    let suppress_tail = if graph.findings_suppressed > 0 {
        format!(
            " · suppressed: {} (see agal.toml)",
            graph.findings_suppressed
        )
    } else {
        String::new()
    };
    let _ = writeln!(
        s,
        "# audiolabs agent summary\n\n\
         **Summary:** Compact structural map of this audio-plugin workspace.  \n\
         Lists plugins, crates, frameworks, migrations, edges, and findings.  \n\
         Use as first context before opening the full JSON graph.\n\n\
         project: **{}**  \n\
         generated: `{}`  \n\
         version: {}  \n\
         health: **{}**  \n\
         nodes: {} · edges: {} · findings: {} (error={} warn={} info={}){}\n",
        graph.project_name,
        graph.generated_at,
        graph.version,
        h,
        graph.nodes.len(),
        graph.edges.len(),
        graph.findings.len(),
        err_n,
        warn_n,
        info_n,
        suppress_tail,
    );

    // Frameworks detected on nodes (not migration config endpoints).
    let used: BTreeSet<&str> = graph.used_frameworks.iter().map(|x| x.as_str()).collect();
    if !used.is_empty() {
        let _ = writeln!(
            s,
            "## frameworks detected\n{}\n",
            used.into_iter().collect::<Vec<_>>().join(", ")
        );
    }

    // Rules
    if !graph.rules.is_empty() {
        let _ = writeln!(s, "## rules");
        for (k, v) in &graph.rules {
            let _ = writeln!(s, "- **{}**: {}", k, v);
        }
        let _ = writeln!(s);
    }

    // Migration — open legacy only; completed work is one quiet line (not a permanent focus).
    write_migration_section(&mut s, &graph.migration_summary);

    // Plugins one-liners
    let _ = writeln!(s, "## plugins");
    for n in graph.nodes.iter().filter(|n| n.kind == "plugin") {
        let _ = writeln!(s, "{}", plugin_line(n));
    }
    let _ = writeln!(s);

    // Crates
    let _ = writeln!(s, "## crates");
    for n in graph.nodes.iter().filter(|n| n.kind == "crate") {
        let _ = writeln!(s, "{}", crate_line(n));
    }
    let _ = writeln!(s);

    // Edges compact by kind
    let _ = writeln!(s, "## edges");
    for kind in edge_kind_order() {
        let group: Vec<&Edge> = graph.edges.iter().filter(|e| e.kind == *kind).collect();
        if group.is_empty() {
            continue;
        }
        let _ = writeln!(s, "### {}", kind);
        for e in group {
            let note = e
                .note
                .as_ref()
                .map(|n| format!(" — {}", n))
                .unwrap_or_default();
            let _ = writeln!(
                s,
                "- `{}` → `{}`{}",
                short_id(&e.from),
                short_id(&e.to),
                note
            );
        }
    }
    let _ = writeln!(s);

    // Findings — agent.md lists error+warn only (info lives in json/html)
    let _ = writeln!(
        s,
        "## findings (error={} warn={} · info={} in json/html)\n\
         health **{}** — if **blocked**, fix errors before feature work.",
        err_n, warn_n, info_n, h,
    );
    let action: Vec<&Finding> = actionable(&graph.findings).collect();
    if action.is_empty() {
        let _ = writeln!(s, "_no error/warn_\n");
    } else {
        for f in action {
            let _ = writeln!(s, "{}", finding_line(f));
        }
        let _ = writeln!(s);
    }

    // Delta (compact)
    if let Some(d) = &graph.delta {
        let _ = writeln!(s, "## delta");
        if d.first_run {
            let _ = writeln!(s, "_first run — no previous graph._\n");
        } else if delta::is_empty(d) {
            let _ = writeln!(s, "_no structural changes since previous graph._\n");
        } else {
            let _ = writeln!(
                s,
                "since `{}`: +{} nodes · -{} nodes · ~{} nodes · +{} edges · -{} edges · +{} findings · -{} resolved\n",
                d.previous_generated_at.as_deref().unwrap_or("?"),
                d.added_nodes.len(),
                d.removed_nodes.len(),
                d.changed_nodes.len(),
                d.added_edges.len(),
                d.removed_edges.len(),
                d.new_findings.len(),
                d.resolved_findings.len(),
            );
            for c in d.changed_nodes.iter().take(8) {
                let _ = writeln!(s, "- **{}**: {}", short_id(&c.id), c.changes.join("; "));
            }
            for f in d.new_findings.iter().take(6) {
                let _ = writeln!(s, "- NEW {}", finding_line(f).trim_start_matches("- "));
            }
            for f in d.resolved_findings.iter().take(4) {
                let _ = writeln!(
                    s,
                    "- RESOLVED [{}] **{}** {}",
                    f.severity,
                    f.code,
                    f.node.as_deref().map(short_id).unwrap_or_default()
                );
            }
            let _ = writeln!(s, "\n_full: `audiolabs.delta.md`_\n");
        }
    }

    // Notes index (focus layer)
    let plugins: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == "plugin")
        .map(|n| n.name.as_str())
        .collect();
    let crates: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == "crate")
        .map(|n| n.name.as_str())
        .collect();
    let _ = writeln!(
        s,
        "## notes (focus)\n`audiolabs/notes/<name>.md` — auto header + human body\n"
    );
    if !plugins.is_empty() {
        let _ = writeln!(
            s,
            "plugins: {}\n",
            plugins
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !crates.is_empty() {
        let _ = writeln!(
            s,
            "crates: {}\n",
            crates
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Skills: short pointer — live index + packs live in AGAL.md
    write_skills_section_compact(&mut s, skills_dir);

    // How to use
    let _ = writeln!(
        s,
        "## read order\n\
         1. **`AGAL.md`** (orientation home — skills + hot path).  \n\
         2. **this file** (structural map + health).  \n\
         3. If health is **blocked**, fix error findings first (path + fix fields).  \n\
         4. **`audiolabs.delta.md`** if present.  \n\
         5. **`notes/<focus>.md`** for the plugin/crate you touch.  \n\
         6. **skills** on demand from the pack list in `AGAL.md` (never dump all).  \n\
         7. Escalate: `agal --plugin NAME .` slice, or `audiolabs.json` for params/edges/info findings.  \n\
         8. HTML is for humans (overview); agents prefer md/json.  \n\
         \n\
         Skills are **not** auto-copied on generate — `agal skills sync` (default: **core** only).  \n\
         Existing `*.slice.json` files are refreshed on every generate; new slices need `--plugin NAME`.  \n\
         Human CLI cheatsheet: **`Cheatsheet.md`** in this folder.\n"
    );

    s
}

/// agal-owned agent entry (`AGAL.md`) — not the user's root AGENTS.md.
///
/// Regenerated by `agal .` and `agal skills sync`. Product rules stay in root AGENTS.md
/// with a one-line pointer here.
pub fn render_agal_md(
    graph: Option<&Audiolabs>,
    skills_dir: Option<&Path>,
    output_dir_name: &str,
) -> String {
    let mut s = String::with_capacity(4096);
    let (project, generated, version, health_s, stats) = match graph {
        Some(g) => {
            let counts = count_by_severity(&g.findings);
            let h = health(&g.findings);
            let err_n = counts.get("error").copied().unwrap_or(0);
            let warn_n = counts.get("warn").copied().unwrap_or(0);
            let info_n = counts.get("info").copied().unwrap_or(0);
            (
                g.project_name.clone(),
                g.generated_at.clone(),
                g.version.clone(),
                h.to_string(),
                format!(
                    "nodes: {} · edges: {} · findings: {} (error={} warn={} info={})",
                    g.nodes.len(),
                    g.edges.len(),
                    g.findings.len(),
                    err_n,
                    warn_n,
                    info_n
                ),
            )
        }
        None => (
            "(unknown — run `agal .`)".into(),
            chrono_stub(),
            env!("CARGO_PKG_VERSION").into(),
            "n/a".into(),
            "map not loaded — run `agal .` for full graph".into(),
        ),
    };

    let _ = writeln!(
        s,
        "# AGAL\n\n\
         **Summary:** agal-owned agent orientation for this workspace.  \n\
         Regenerated by `agal .` and `agal skills sync` — **do not hand-edit**.  \n\
         Put product / team rules in the workspace-root **`AGENTS.md`** and point here.\n\n\
         project: **{project}**  \n\
         generated: `{generated}`  \n\
         tool: agal {version}  \n\
         health: **{health_s}**  \n\
         {stats}  \n\
         output: `{out}/`\n",
        project = project,
        generated = generated,
        version = version,
        health_s = health_s,
        stats = stats,
        out = output_dir_name,
    );

    let _ = writeln!(
        s,
        "## Root AGENTS.md (yours)\n\n\
         agal **never** overwrites the workspace-root `AGENTS.md`.  \n\
         Minimal hook (keep your own rules above/below):\n\n\
         ```markdown\n\
         ## Orientation (agal)\n\
         Read **`{out}/AGAL.md`** first for map, health, and skills.\n\
         ```\n",
        out = output_dir_name,
    );

    let _ = writeln!(
        s,
        "## Hot path\n\n\
         1. **this file** (`AGAL.md`) — skills index + orientation  \n\
         2. **`audiolabs.agent.md`** — structural map + health detail  \n\
         3. If **blocked** — fix error findings first (`path` + `fix`)  \n\
         4. **`audiolabs.delta.md`** if present  \n\
         5. **`notes/<focus>.md`** for the plugin/crate you touch  \n\
         6. **skills** on demand (list below) — never dump the whole tree  \n\
         7. Escalate: `agal --plugin NAME .` → slice, or `audiolabs.json`  \n\
         8. HTML / Cheatsheet are for humans\n"
    );

    if let Some(g) = graph {
        write_migration_section(&mut s, &g.migration_summary);
    }

    write_skills_packs_and_index(&mut s, skills_dir);

    let _ = writeln!(
        s,
        "## Artifacts in this folder\n\n\
         | Path | Role |\n\
         |------|------|\n\
         | **`AGAL.md`** | **AI entry** (this file) |\n\
         | `audiolabs.agent.md` | structural map + health |\n\
         | `audiolabs.delta.md` | structural diff |\n\
         | `audiolabs.json` | full edges / params / info findings |\n\
         | `notes/` | auto header + human body |\n\
         | `skills/` | synced tool packs (see above) |\n\
         | `Cheatsheet.md` | human CLI guide |\n\
         | `audiolabs.html` | human graph |\n\
         | `*.slice.json` | 1-hop plugin focus |\n"
    );

    let _ = writeln!(
        s,
        "## Commands\n\n\
         ```bash\n\
         agal .                              # refresh map + AGAL.md + notes + html\n\
         agal --plugin aether .              # + one-hop slice\n\
         agal skills sync                    # default: core (00-core)\n\
         agal skills sync --only ui/slint    # single pack file\n\
         agal skills sync --only policy,agents\n\
         agal skills list\n\
         agal doctor                         # Clippy + clap-validator on PATH\n\
         ```\n"
    );

    s
}

/// Write `{output_dir}/AGAL.md`. `graph` optional (skills-sync may only have JSON on disk).
pub fn write_agal_md(
    output_dir: &Path,
    graph: Option<&Audiolabs>,
    output_dir_name: &str,
) -> Result<(), String> {
    let skills_dir = output_dir.join("skills");
    let skills = if skills_dir.is_dir() {
        Some(skills_dir.as_path())
    } else {
        None
    };
    let body = render_agal_md(graph, skills, output_dir_name);
    let path = output_dir.join("AGAL.md");
    fs::write(&path, body).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    Ok(())
}

/// After `skills sync`: refresh AGAL.md using `audiolabs.json` if present.
pub fn refresh_agal_after_skills_sync(
    project_root: &Path,
    output_dir_name: &str,
) -> Result<(), String> {
    let output_dir = project_root.join(output_dir_name);
    fs::create_dir_all(&output_dir)
        .map_err(|e| format!("cannot create {}: {}", output_dir.display(), e))?;
    let json_path = output_dir.join("audiolabs.json");
    let graph = crate::delta::load_previous(&json_path).map(|mut g| {
        g.delta = None;
        g
    });
    write_agal_md(&output_dir, graph.as_ref(), output_dir_name)?;
    println!("  agal: {}/AGAL.md (skills index refreshed)", output_dir_name);
    Ok(())
}

fn chrono_stub() -> String {
    // Avoid pulling chrono into core — generate() always has a real stamp.
    "skills-sync".into()
}

fn write_migration_section(s: &mut String, ms: &crate::MigrationSummary) {
    if ms.migrations.is_empty() {
        return;
    }
    if ms.total_legacy > 0 {
        let _ = writeln!(
            s,
            "## migration (open)\nplugins: {} · **legacy: {}** · migrated: {}\n",
            ms.total_plugins, ms.total_legacy, ms.total_migrated
        );
        for (id, d) in &ms.migrations {
            if d.legacy_count > 0 {
                let _ = writeln!(
                    s,
                    "- **{}** `{}`→`{}`: {} legacy ({})",
                    id,
                    d.from,
                    d.to,
                    d.legacy_count,
                    d.legacy_plugins.join(", ")
                );
            }
        }
        let _ = writeln!(s);
        return;
    }
    // Done — one quiet line, no per-plugin migration theatre.
    if ms.total_migrated > 0 {
        let _ = writeln!(
            s,
            "## migration\n_complete_ — {} plugins on target adapters (no open legacy).\n",
            ms.total_migrated
        );
    }
}

fn write_skills_section_compact(s: &mut String, skills_dir: Option<&Path>) {
    let _ = writeln!(s, "## skills\n");
    let _ = writeln!(
        s,
        "Full pack list + index: **`AGAL.md`**. Load on demand only.\n"
    );
    let n = skills_dir
        .filter(|d| d.is_dir())
        .map(index_skill_files)
        .map(|v| v.len())
        .unwrap_or(0);
    if n == 0 {
        let _ = writeln!(
            s,
            "_no skills synced yet — `agal skills sync` (default core)._\n"
        );
    } else {
        let _ = writeln!(s, "_{n} skill file(s) under `skills/` — see AGAL.md._\n");
    }
}

fn write_skills_packs_and_index(s: &mut String, skills_dir: Option<&Path>) {
    let _ = writeln!(
        s,
        "## Skills (tool packs)\n\n\
         Canonical packs live in the **agal tool**. Sync a curated copy into `skills/`:\n\n\
         | Group | Path | Sync |\n\
         |-------|------|------|\n\
         | **core** (default) | `skills/00-core/` | `agal skills sync` |\n\
         | policy | `skills/01-policy/` | `--only policy` |\n\
         | frameworks | `skills/02-frameworks/` | `--only frameworks` |\n\
         | formats | `skills/03-formats/` | `--only formats` |\n\
         | ui | `skills/04-ui/` | `--only ui` or `ui/slint` |\n\
         | migrations | `skills/05-migrations/` | `--only migrations` |\n\
         | agents | `skills/06-agents/` | `--only agents` |\n\n\
         - **Load on demand** — never dump `skills/` into context.\n\
         - Without `--force`, existing files are **skipped** (local edits kept).\n\
         - Prefer adapting pack files in place; do **not** invent root `*_SKILL.md`.\n\
         - Optional frontmatter: `id`, `summary` (≤120 chars), `adapted: true`.\n"
    );

    let Some(dir) = skills_dir.filter(|d| d.is_dir()) else {
        let _ = writeln!(
            s,
            "### present\n\n_no `skills/` yet — run `agal skills sync`._\n"
        );
        return;
    };
    let index = index_skill_files(dir);
    if index.is_empty() {
        let _ = writeln!(
            s,
            "### present\n\n_`skills/` empty — run `agal skills sync` (core)._\n"
        );
        return;
    }
    let _ = writeln!(s, "### present (id · summary · path)\n");
    for entry in index {
        let _ = writeln!(
            s,
            "- **{}** — {} · [`{}`](./{})",
            entry.id, entry.summary, entry.rel_path, entry.rel_path
        );
    }
    let _ = writeln!(s);
}

#[derive(Debug)]
struct SkillIndexEntry {
    id: String,
    summary: String,
    rel_path: String,
}

fn index_skill_files(skills_dir: &Path) -> Vec<SkillIndexEntry> {
    let mut out = Vec::new();
    let walker = walkdir::WalkDir::new(skills_dir)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.')
        });
    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(rel) = path.strip_prefix(skills_dir).ok() else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        // Skip folder readmes / authoring meta noise if any
        if rel_str.eq_ignore_ascii_case("README.md") {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let (id, summary) = parse_skill_frontmatter(&text, &rel_str);
        out.push(SkillIndexEntry {
            id,
            summary,
            rel_path: format!("skills/{}", rel_str),
        });
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

/// Parse YAML-ish frontmatter `id` + `summary`; fallback from path/title.
fn parse_skill_frontmatter(text: &str, rel_path: &str) -> (String, String) {
    let fallback_id = Path::new(rel_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel_path.to_string());
    let mut id = fallback_id.clone();
    let mut summary = String::new();

    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            for line in fm.lines() {
                let line = line.trim();
                if let Some(v) = line.strip_prefix("id:") {
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        id = v.to_string();
                    }
                } else if let Some(v) = line.strip_prefix("summary:") {
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if !v.is_empty() {
                        summary = v.to_string();
                    }
                }
            }
        }
    }
    if summary.is_empty() {
        // First markdown heading or first non-empty line after frontmatter
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('#') {
                summary = t.trim_start_matches('#').trim().to_string();
                break;
            }
        }
        if summary.is_empty() {
            summary = "(no summary)".into();
        }
    }
    // Keep index token-cheap
    if summary.len() > 120 {
        summary.truncate(117);
        summary.push_str("...");
    }
    (id, summary)
}

fn plugin_line(n: &Node) -> String {
    // Only surface migration status when open work remains (legacy / unknown).
    let mig_badge = match n.migration_status.as_deref() {
        Some("legacy") => " [legacy]",
        Some("unknown") => " [mig=?]",
        _ => "",
    };
    let fw = n.frameworks.join("+");
    let mut extras = Vec::new();
    if let Some(ast) = &n.ast_summary {
        if let Some(logic) = ast.plugin_logic_impls.iter().next() {
            extras.push(format!("logic={}", logic));
        }
        if let Some(params) = ast.params_structs.iter().next() {
            let nfields = ast.params_fields.get(params).map(|f| f.len()).unwrap_or(0);
            extras.push(format!("params={}({})", params, nfields));
        }
        if !ast.ipc_signals.is_empty() {
            extras.push(format!(
                "ipc={}",
                ast.ipc_signals
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        if !ast.slint_components.is_empty() {
            extras.push(format!("slint_comp={}", ast.slint_components.len()));
        }
        let roles = ast.role_tags();
        if !roles.is_empty() {
            extras.push(format!(
                "roles={}",
                roles.into_iter().collect::<Vec<_>>().join(",")
            ));
        }
    }
    let deps: Vec<String> = n.internal_deps.iter().map(|d| short_id(d)).collect();
    format!(
        "- **{}** `{}`{} fw=`{}` deps=[{}] {}",
        n.name,
        n.id,
        mig_badge,
        fw,
        deps.join(", "),
        extras.join(" ")
    )
}

fn crate_line(n: &Node) -> String {
    let mut extras = Vec::new();
    if let Some(ast) = &n.ast_summary {
        if !ast.public_api.is_empty() {
            extras.push(format!("api={}", ast.public_api.len()));
        }
        if !ast.slint_exports.is_empty() {
            extras.push(format!("slint_export={}", ast.slint_exports.len()));
        }
        if !ast.ipc_signals.is_empty() {
            extras.push(format!(
                "ipc={}",
                ast.ipc_signals
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        if ast.process_method_count > 0 {
            extras.push(format!("process_methods={}", ast.process_method_count));
        }
    }
    let deps: Vec<String> = n.internal_deps.iter().map(|d| short_id(d)).collect();
    format!(
        "- **{}** `{}` deps=[{}] {}",
        n.name,
        n.id,
        deps.join(", "),
        extras.join(" ")
    )
}

fn finding_line(f: &Finding) -> String {
    let icon = match f.severity {
        Severity::Error => "ERR",
        Severity::Warn => "WRN",
        Severity::Info => "INF",
    };
    let node = f
        .node
        .as_ref()
        .map(|n| format!(" `{}`", short_id(n)))
        .unwrap_or_default();
    let mut line = format!("- [{}] **{}**{}: {}", icon, f.code, node, f.message);
    if let Some(path) = &f.path {
        line.push_str(&format!(" · path=`{}`", path));
    }
    if let Some(fix) = &f.fix {
        line.push_str(&format!(" · fix: {}", fix));
    }
    line
}

fn short_id(id: &str) -> String {
    id.trim_start_matches("plugins/")
        .trim_start_matches("crates/")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{parse_skill_frontmatter, render_agal_md};

    #[test]
    fn frontmatter_id_and_summary() {
        let text = "---\nid: slint-lx\ngroup: ui\nsource: workspace\nsummary: hello world skill\n---\n\n# Title\n";
        let (id, summary) = parse_skill_frontmatter(text, "04-ui/slint.md");
        assert_eq!(id, "slint-lx");
        assert_eq!(summary, "hello world skill");
    }

    #[test]
    fn frontmatter_fallback_from_path() {
        let (id, summary) = parse_skill_frontmatter("# Only heading\n", "04-ui/slint.md");
        assert_eq!(id, "slint");
        assert_eq!(summary, "Only heading");
    }

    #[test]
    fn agal_md_lists_skills_without_legacy_paths() {
        let md = render_agal_md(None, None, "audiolabs");
        assert!(md.contains("# AGAL"));
        assert!(md.contains("00-core"));
        assert!(md.contains("04-ui"));
        assert!(!md.contains("10-lx"));
        assert!(!md.contains("90-project"));
    }
}

fn edge_kind_order() -> &'static [&'static str] {
    &[
        "depends_on",
        "build_depends_on",
        "dev_depends_on",
        "uses_ui",
        "ipc_peer",
        "runtime_depends_on",
    ]
}

/// JSON slice: one node + 1-hop edges + related findings.
pub fn plugin_slice(graph: &Audiolabs, plugin_name: &str) -> Option<serde_json::Value> {
    let node = graph.nodes.iter().find(|n| {
        n.name == plugin_name || n.id == plugin_name || n.id.ends_with(&format!("/{}", plugin_name))
    })?;
    let id = &node.id;
    let related_edges: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|e| e.from == *id || e.to == *id)
        .collect();
    let neighbor_ids: BTreeSet<&str> = related_edges
        .iter()
        .flat_map(|e| [e.from.as_str(), e.to.as_str()])
        .collect();
    let neighbors: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| neighbor_ids.contains(n.id.as_str()) && n.id != *id)
        .collect();
    let findings: Vec<&Finding> = graph
        .findings
        .iter()
        .filter(|f| f.node.as_deref() == Some(id.as_str()))
        .collect();

    Some(serde_json::json!({
        "plugin": node,
        "neighbors": neighbors,
        "edges": related_edges,
        "findings": findings,
    }))
}
