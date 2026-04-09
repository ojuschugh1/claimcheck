mod cli;
mod extractor;
mod git;
mod lockfile;
mod parser;
mod reporter;
mod scorer;
mod types;
mod verifier;

use clap::Parser;
use cli::Cli;
use verifier::VerifierConfig;

fn main() {
    let cli = Cli::parse();

    let project_dir = cli.project_dir.unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|e| {
            eprintln!("Error: could not determine current directory: {}", e);
            std::process::exit(1);
        })
    });

    if cli.verbose {
        eprintln!("[verbose] project dir: {}", project_dir.display());
        eprintln!("[verbose] transcript: {}", cli.transcript.display());
        eprintln!("[verbose] baseline: {}", cli.baseline);
    }

    let messages = match parser::parse_transcript(&cli.transcript) {
        Ok(msgs) => msgs,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if cli.verbose {
        eprintln!("[verbose] parsed {} assistant message(s)", messages.len());
    }

    let claims = extractor::extract_claims(&messages);

    if messages.is_empty() || claims.is_empty() {
        println!("No verifiable claims found");
        std::process::exit(0);
    }

    if cli.verbose {
        eprintln!("[verbose] extracted {} claim(s)", claims.len());
    }

    if cli.show_claims {
        println!("{}", reporter::format_claims_list(&claims));
    }

    let config = VerifierConfig {
        project_dir,
        baseline: cli.baseline,
        retest: cli.retest,
        test_cmd: cli.test_cmd,
        verbose: cli.verbose,
        transcript_text: Some(
            messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        ),
    };

    let verified = verifier::verify_claims(&claims, &config);
    let score = scorer::calculate_score(&verified);

    if cli.json {
        println!("{}", reporter::format_json_report(&score, &verified));
    } else {
        println!("{}", reporter::format_text_report(&score, &verified));
    }
}
