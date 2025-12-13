use chat_cli::agent::subagent_widget_demo;
use clap::Parser;

/// Subagent widget demo - demonstrates running multiple subagents concurrently
///
/// This tool allows you to run multiple subagent queries simultaneously and
/// visualize their progress in real-time using an interactive widget interface.
#[derive(Parser, Debug)]
#[command(name = "subagent_widget")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Agent queries in the format: agent_name query agent_name query ...
    ///
    /// Each pair consists of an agent name followed by the query to send to that agent.
    /// You can specify multiple agent/query pairs to run them concurrently.
    ///
    /// Example: subagent_widget agent1 "What is Rust?" agent2 "Explain async/await"
    #[arg(required = true, num_args = 2..)]
    queries: Vec<String>,
}

fn main() {
    let args = Args::parse();

    // Parse the queries into (agent_name, query) pairs
    let mut agent_queries: Vec<(String, String)> = Vec::new();

    for chunk in args.queries.chunks(2) {
        if chunk.len() == 2 {
            agent_queries.push((chunk[0].clone(), chunk[1].clone()));
        }
    }

    if agent_queries.is_empty() {
        eprintln!("Error: You must provide at least one agent name and query pair.");
        eprintln!("Usage: subagent_widget <agent_name> <query> [<agent_name> <query> ...]");
        panic!()
    }

    subagent_widget_demo(agent_queries);
}
