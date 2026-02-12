use std::collections::HashSet;

use crate::llm::LlmClient;
use crate::team::TeamOutput;

#[derive(Debug, Clone, Default)]
pub struct KnowledgeBase {
    insights: Vec<String>,
    seen: HashSet<String>,
    max_insights: usize,
}

impl KnowledgeBase {
    pub fn new(max_insights: usize) -> Self {
        Self {
            insights: Vec::new(),
            seen: HashSet::new(),
            max_insights,
        }
    }

    pub async fn extract_and_store(
        &mut self,
        agent_outputs: &[(String, String)],
        problem: &str,
        llm: &LlmClient,
        max_tokens: u32,
    ) {
        let mut summary = String::new();
        for (name, content) in agent_outputs {
            let excerpt: String = content.chars().take(600).collect();
            summary.push_str(&format!("--- {} ---\n{}\n\n", name, excerpt));
        }

        if summary.is_empty() {
            return;
        }

        let user_msg = format!(
            "Analyse these AI agent responses to the problem below. \
             Extract 1-3 KEY INSIGHTS that are novel, non-obvious, or represent \
             important logical breakthroughs. Focus on:\n\
             - Unique logical deductions other agents missed\n\
             - Important edge cases or constraints identified\n\
             - Methodological approaches that seem promising\n\
             - Counter-examples or contradictions discovered\n\n\
             **Problem:** {problem}\n\n\
             **Agent Outputs:**\n{summary}\n\n\
             Respond with ONLY a JSON array of short insight strings, e.g.:\n\
             [\"Insight 1\", \"Insight 2\"]",
        );

        let resp = llm
            .chat_completion(
                "You are a knowledge curator. Extract the most valuable non-obvious \
                 insights from AI agent outputs. Output only valid JSON.",
                &user_msg,
                0.2,
                1.0,
                max_tokens.min(512),
            )
            .await;

        if let Ok(resp) = resp {
            self.parse_and_add(&resp.content);
        }
    }

    pub fn extract_from_critique(&mut self, critique: &str) {
        if critique.is_empty() || critique.len() < 20 {
            return;
        }

        for sentence in critique.split(['.', '!']) {
            let sentence = sentence.trim();
            if sentence.len() > 30 && sentence.len() < 200 {
                let lower = sentence.to_lowercase();
                let describes_problem = lower.contains("flaw")
                    || lower.contains("incorrect")
                    || lower.contains("wrong")
                    || lower.contains("miss")
                    || lower.contains("overlook")
                    || lower.contains("contradict")
                    || lower.contains("gap")
                    || lower.contains("error")
                    || lower.contains("failed to")
                    || lower.contains("should have")
                    || lower.contains("did not consider")
                    || lower.contains("not account")
                    || lower.contains("alternative");
                let is_praise = lower.starts_with("good")
                    || lower.starts_with("correct")
                    || lower.starts_with("well done")
                    || lower.starts_with("the agents correctly")
                    || lower.starts_with("strong");
if describes_problem && !is_praise {
                    let warning = format!("UNRESOLVED: {}", sentence);
                    self.add(warning);
                }
            }
        }
    }

    pub fn extract_conflicts(&mut self, team_outputs: &[(String, &TeamOutput)]) {
        if team_outputs.len() < 2 {
            return;
        }

        let mut conclusions: Vec<(String, String)> = Vec::new();

        for (team_name, output) in team_outputs {
            if let Some(ao) = output
                .agent_outputs
                .iter()
                .find(|a| a.strategy != crate::genotype::ReasoningStrategy::RedTeam)
            {
                let content = &ao.content;
                let tail: String = content
                    .chars()
                    .rev()
                    .take(300)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                conclusions.push((team_name.clone(), tail.to_lowercase()));
            }
        }

        for i in 0..conclusions.len() {
            for j in (i + 1)..conclusions.len() {
                let words_a: HashSet<&str> = conclusions[i].1.split_whitespace().collect();
                let words_b: HashSet<&str> = conclusions[j].1.split_whitespace().collect();
                let union = words_a.union(&words_b).count() as f64;
                let intersection = words_a.intersection(&words_b).count() as f64;
                let jaccard = if union > 0.0 { intersection / union } else { 0.0 };

                if jaccard < 0.25 {
                    let conflict = format!(
                        "CONFLICT: Team \"{}\" and Team \"{}\" reached very different \
                         conclusions. Investigate which is correct and why.",
                        conclusions[i].0, conclusions[j].0,
                    );
                    self.add(conflict);
                }
            }
        }
    }

    pub fn add(&mut self, insight: String) {
        let normalised = insight.to_lowercase().trim().to_string();
        if normalised.is_empty() || self.seen.contains(&normalised) {
            return;
        }
        self.seen.insert(normalised);
        self.insights.push(insight);

        while self.insights.len() > self.max_insights {
            if let Some(removed) = self.insights.first() {
                let key = removed.to_lowercase().trim().to_string();
                self.seen.remove(&key);
            }
            self.insights.remove(0);
        }
    }

    pub fn hints(&self) -> &[String] {
        &self.insights
    }

    fn parse_and_add(&mut self, response: &str) {
        if let Ok(arr) = serde_json::from_str::<Vec<String>>(response) {
            for s in arr {
                self.add(s);
            }
            return;
        }

        let trimmed = response.trim();
        if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') {
                let json_str = &trimmed[start..=end];
                if let Ok(arr) = serde_json::from_str::<Vec<String>>(json_str) {
                    for s in arr {
                        self.add(s);
                    }
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.insights.is_empty()
    }

    pub fn len(&self) -> usize {
        self.insights.len()
    }
}
