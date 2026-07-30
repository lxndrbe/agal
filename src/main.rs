use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "agal",
    version,
    about = "Audio-plugin workspace orientation: graph, notes, curated skills"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Workspace root (when generating without subcommand)
    #[arg(default_value = ".")]
    project_root: PathBuf,

    #[arg(short, long)]
    watch: bool,

    #[arg(long)]
    install_hook: bool,

    #[arg(short, long)]
    output: Option<String>,

    #[arg(short, long)]
    plugin: Option<String>,

    #[arg(short, long)]
    verbose: bool,

    #[arg(long)]
    agent_only: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Curated skill packs (live in the tool; sync into workspace on demand)
    Skills {
        #[command(subcommand)]
        action: SkillsCmd,
    },
    /// Check Clippy / clap-validator on PATH and print recommended commands
    Doctor {
        #[arg(default_value = ".")]
        project_root: PathBuf,
    },
}

#[derive(Subcommand)]
enum SkillsCmd {
    /// List embedded skills and groups
    List,
    /// Copy selected groups/skills into <workspace>/<output>/skills/
    Sync {
        /// Comma list: groups (`policy`, `ui`) and/or singles (`ui/slint`, `04-ui/slint`); default `core`
        #[arg(long, default_value = "core")]
        only: String,
        /// Overwrite existing skill files
        #[arg(long)]
        force: bool,
        /// Output dir under project root (default: audiolabs)
        #[arg(short, long)]
        output: Option<String>,
        #[arg(default_value = ".")]
        project_root: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Skills { action } => match action {
                SkillsCmd::List => {
                    audiolabs_core::skills::print_list();
                    return;
                }
                SkillsCmd::Sync {
                    only,
                    force,
                    output,
                    project_root,
                } => {
                    let root = match project_root.canonicalize() {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!(
                                "error: cannot canonicalize {}: {}",
                                project_root.display(),
                                e
                            );
                            std::process::exit(1);
                        }
                    };
                    if !root.join("Cargo.toml").exists() {
                        eprintln!("error: no Cargo.toml in {}", root.display());
                        std::process::exit(1);
                    }
                    let selection = match audiolabs_core::skills::parse_selection(&only) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("error: {}", e);
                            std::process::exit(1);
                        }
                    };
                    let output_dir = output.unwrap_or_else(|| "audiolabs".to_string());
                    let opts = audiolabs_core::skills::SyncOptions {
                        selection,
                        force,
                        output_dir,
                    };
                    if let Err(e) = audiolabs_core::skills::sync(&root, &opts) {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                    return;
                }
            },
            Commands::Doctor { project_root } => {
                let root = match project_root.canonicalize() {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!(
                            "error: cannot canonicalize {}: {}",
                            project_root.display(),
                            e
                        );
                        std::process::exit(1);
                    }
                };
                if !root.join("Cargo.toml").exists() {
                    eprintln!("error: no Cargo.toml in {}", root.display());
                    std::process::exit(1);
                }
                match audiolabs_core::doctor(&root) {
                    Ok(report) => {
                        print!("{report}");
                        return;
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    let project_root = match cli.project_root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "error: cannot canonicalize {}: {}",
                cli.project_root.display(),
                e
            );
            std::process::exit(1);
        }
    };

    if !project_root.join("Cargo.toml").exists() {
        eprintln!("error: no Cargo.toml found in {}", project_root.display());
        std::process::exit(1);
    }

    let options = audiolabs_core::GenerateOptions {
        watch_mode: cli.watch,
        install_hook: cli.install_hook,
        output_dir_override: cli.output,
        verbose: cli.verbose,
        agent_only: cli.agent_only,
        plugin_filter: cli.plugin,
    };

    if let Err(e) = audiolabs_core::generate(&project_root, &options) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
