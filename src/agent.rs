use std::time::Duration;
use uuid::Uuid;

use crate::config::Config;
use crate::genotype::{Genotype, ReasoningStrategy};
use crate::llm::LlmClient;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: Uuid,
    pub genotype: Genotype,
}

impl Agent {
    pub fn new(genotype: Genotype) -> Self {
        Self {
            id: Uuid::new_v4(),
            genotype,
        }
    }

    pub async fn execute(
        &self,
        problem: &str,
        llm: &LlmClient,
        config: &Config,
    ) -> Result<AgentOutput> {
        let system_prompt = self.genotype.build_system_prompt();
        let start = std::time::Instant::now();

        let response = llm
            .chat_completion(
                &system_prompt,
                problem,
                self.genotype.temperature,
                self.genotype.top_p,
                config.max_tokens,
            )
            .await?;

        let elapsed = start.elapsed();

        Ok(AgentOutput {
            agent_id: self.id,
            genotype_name: self.genotype.name.clone(),
            strategy: self.genotype.strategy.clone(),
            content: response.content,
            tokens_used: response.total_tokens,
            elapsed,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub agent_id: Uuid,
    pub genotype_name: String,
    pub strategy: ReasoningStrategy,
    pub content: String,
    pub tokens_used: u32,
    pub elapsed: Duration,
}
