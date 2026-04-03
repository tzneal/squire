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
            if cli.json {
                if e.starts_with('{') {
                    println!("{e}");
                } else {
                    println!(
                        "{}",
                        serde_json::to_string(&squire::response::ErrorResult { error: e.clone() })
                            .unwrap()
                    );
                }
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(1);
        }
    }
}
