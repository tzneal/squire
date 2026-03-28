use clap::Parser;
use squire::cli::Cli;

fn main() {
    let cli = Cli::parse();
    if cli.llm_help {
        print!("{}", squire::cli::LLM_HELP);
        return;
    }
    let Some(ref command) = cli.command else {
        eprintln!("error: a subcommand is required (use --help or --llm-help)");
        std::process::exit(1);
    };
    let dir = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("error: cannot determine current directory: {e}");
        std::process::exit(1);
    });
    match squire::run(&cli, command, &dir) {
        Ok(out) => {
            eprint!("{}", out.stderr);
            print!("{}", out.stdout);
        }
        Err(e) => {
            if e.starts_with('{') {
                // Structured error (e.g. conflict report) — format appropriately.
                if cli.json {
                    println!("{e}");
                } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(&e) {
                    if v["conflict"].as_bool() == Some(true) {
                        eprintln!("Conflict during rebase:");
                        if let Some(files) = v["conflicting_files"].as_array() {
                            for f in files {
                                eprintln!(
                                    "  {}: {}",
                                    f["status"].as_str().unwrap_or("unknown"),
                                    f["file"].as_str().unwrap_or("?")
                                );
                            }
                        }
                        if let Some(hint) = v["hint"].as_str() {
                            eprintln!("{hint}");
                        }
                    } else {
                        eprintln!("error: {e}");
                    }
                } else {
                    eprintln!("error: {e}");
                }
            } else if cli.json {
                println!("{}", serde_json::json!({ "error": e }));
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(1);
        }
    }
}
