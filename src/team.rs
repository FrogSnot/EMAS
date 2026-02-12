use std::time::Duration;
use uuid::Uuid;

use anyhow::Result;
use futures::future::join_all;

use crate::agent::{Agent, AgentOutput};
use crate::config::Config;
use crate::llm::LlmClient;

#[derive(Debug, Clone)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub agents: Vec<Agent>,
    pub generation: usize,
}

impl Team {
    pub async fn execute(
        &self,
        problem: &str,
        llm: &LlmClient,
        config: &Config,
    ) -> Result<TeamOutput> {
        let (standard, red_team): (Vec<_>, Vec<_>) =
            self.agents.iter().partition(|a| !a.genotype.is_red_team);

        let standard_futures: Vec<_> = standard
            .iter()
            .map(|agent| agent.execute(problem, llm, config))
            .collect();

        let standard_results = join_all(standard_futures).await;

        let mut agent_outputs: Vec<AgentOutput> = Vec::with_capacity(self.agents.len());
        let mut warnings: Vec<String> = Vec::new();
        for res in standard_results {
            match res {
                Ok(output) => agent_outputs.push(output),
                Err(e) => {
                    let msg = format!("Agent failed in {}: {e:#}", self.name);
                    tracing::warn!(team = %self.name, "Agent failed: {e:#}");
                    warnings.push(msg);
                }
            }
        }

        if !red_team.is_empty() && !agent_outputs.is_empty() {
            let mut others_summary = String::new();
            for ao in &agent_outputs {
                others_summary.push_str(&format!(
                    "--- {} ({}) ---\n{}\n\n",
                    ao.genotype_name, ao.strategy, ao.content,
                ));
            }

            let red_prompt = format!(
                "{}\n\n\
                 === Other Agents' Responses (Your Task: Find Flaws) ===\n\
                 The following agents on your team have already answered. \
                 Your job is to rigorously check their logic, find any \
                 contradictions, false assumptions, or errors, and present \
                 a corrected analysis if needed.\n\n{}",
                problem, others_summary,
            );

            let red_futures: Vec<_> = red_team
                .iter()
                .map(|agent| agent.execute(&red_prompt, llm, config))
                .collect();

            let red_results = join_all(red_futures).await;
            for res in red_results {
                match res {
                    Ok(output) => agent_outputs.push(output),
                    Err(e) => {
                        let msg = format!("Red-team agent failed in {}: {e:#}", self.name);
                        tracing::warn!(team = %self.name, "Red-team agent failed: {e:#}");
                        warnings.push(msg);
                    }
                }
            }
        }

        let total_tokens: u32 = agent_outputs.iter().map(|o| o.tokens_used).sum();
        let total_elapsed: Duration = agent_outputs
            .iter()
            .map(|o| o.elapsed)
            .max()
            .unwrap_or_default();

        Ok(TeamOutput {
            team_id: self.id,
            team_name: self.name.clone(),
            agent_outputs,
            total_tokens,
            total_elapsed,
            warnings,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TeamOutput {
    pub team_id: Uuid,
    pub team_name: String,
    pub agent_outputs: Vec<AgentOutput>,
    pub total_tokens: u32,
    pub total_elapsed: Duration,
    pub warnings: Vec<String>,
}
