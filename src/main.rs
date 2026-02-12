use anyhow::Result;
use clap::Parser;
use colored::*;

use emas::arena::Arena;
use emas::config::{Cli, Config};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    if cli.tui {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
            )
            .with_target(false)
            .with_writer(std::io::sink)
            .init();

        return emas::tui::run_tui(&cli).await;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let problem = cli
        .problem
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Problem is required in CLI mode. Use --tui for interactive mode."))?;

    let config = Config::from_cli(&cli)?;
    let arena = Arena::new(config);
    let result = arena.run(problem).await?;

    println!();
    println!(
        "{}",
        "========================================================"
            .bold()
            .bright_white()
    );
    println!(
        "{}  {} (score {}/10)",
        "WINNING TEAM:".bold().yellow(),
        result.best_team.name.green().bold(),
        format!("{:.2}", result.best_score.total).green().bold(),
    );
    println!(
        "   Generations run: {}",
        result.generations_run.to_string().cyan(),
    );
    println!();

    println!("{}", "   Team composition:".bold());
    for (i, agent) in result.best_team.agents.iter().enumerate() {
        let is_last = i == result.best_team.agents.len() - 1;
        let branch = if is_last { "|--" } else { "|--" };
        println!(
            "   {} {} ({}, temp {:.2})",
            branch,
            agent.genotype.name.white().bold(),
            agent.genotype.strategy.to_string().dimmed(),
            agent.genotype.temperature,
        );
    }

    println!();
    println!(
        "{}",
        "------------------------------------------------------"
            .dimmed()
    );
    println!("{}", "SYNTHESISED RESPONSE:".bold().bright_white());
    println!(
        "{}",
        "------------------------------------------------------"
            .dimmed()
    );
    println!();
    println!("{}", result.synthesis);
    println!();
    println!(
        "{}",
        "========================================================"
            .bold()
            .bright_white()
    );

    Ok(())
}
