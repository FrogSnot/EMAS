use std::collections::HashSet;

use anyhow::Result;
use tracing::debug;

use crate::config::Config;
use crate::llm::LlmClient;
use crate::team::TeamOutput;

#[derive(Debug, Clone)]
pub struct FitnessScore {
    pub quality: f64,
    pub consistency: f64,
    pub efficiency: f64,
    pub diversity_penalty: f64,
    pub total: f64,
    pub judge_critique: String,
}

impl std::fmt::Display for FitnessScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:.2}  (Q:{:.1} C:{:.1} E:{:.1}{})",
            self.total,
            self.quality,
            self.consistency,
            self.efficiency,
            if self.diversity_penalty > 0.01 {
                format!(" D:-{:.1}", self.diversity_penalty)
            } else {
                String::new()
            },
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConclusionHistory {
    entries: Vec<(HashSet<String>, f64)>,
}

impl ConclusionHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, output: &TeamOutput, score: f64) {
        let fp = Self::fingerprint(output);
        self.entries.push((fp, score));
    }

    pub fn penalty(&self, output: &TeamOutput) -> f64 {
        if self.entries.is_empty() {
            return 0.0;
        }
        let fp = Self::fingerprint(output);
        if fp.is_empty() {
            return 0.0;
        }

        let mut max_penalty = 0.0_f64;
        for (prev_fp, prev_score) in &self.entries {
            let union = fp.union(prev_fp).count() as f64;
            let intersection = fp.intersection(prev_fp).count() as f64;
            let similarity = if union > 0.0 { intersection / union } else { 0.0 };

            if similarity > 0.6 && *prev_score < 7.0 {
                let badness = (7.0 - prev_score).max(0.0) / 6.0;
                let sim_factor = (similarity - 0.6) / 0.4;
                let penalty = 3.0 * badness * sim_factor;
                max_penalty = max_penalty.max(penalty);
            }
        }

        max_penalty
    }

    fn fingerprint(output: &TeamOutput) -> HashSet<String> {
        let mut words = HashSet::new();
        for ao in &output.agent_outputs {
            if ao.strategy == crate::genotype::ReasoningStrategy::RedTeam {
                continue;
            }
            let tail: String = ao.content.chars().rev().take(200).collect::<String>()
                .chars().rev().collect();
            for w in tail.to_lowercase().split_whitespace() {
                let clean: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
                if clean.len() > 2 {
                    words.insert(clean);
                }
            }
        }
        words
    }
}

struct JudgePersona {
    name: &'static str,
    system_prompt: &'static str,
    extra_criteria: &'static str,
}

const JUDGE_PERSONAS: &[JudgePersona] = &[
    JudgePersona {
        name: "Correctness Judge",
        system_prompt: "You are a precise, strict evaluation judge. \
                        Your sole focus is on logical and factual correctness. \
                        Output only valid JSON.",
        extra_criteria: "\
             6. Logical Validity - Does every deductive step actually follow? \
                Are there any logical fallacies or unjustified leaps?\n\
             7. Alternative Solutions - Did the agents check whether other \
                valid solutions exist, or did they prematurely commit to one?",
    },
    JudgePersona {
        name: "Skeptical Judge",
        system_prompt: "You are a deeply skeptical evaluation judge. \
                        You assume every answer is wrong until proven right. \
                        A confident, well-formatted answer that is WRONG should \
                        score LOWER than a messy answer that is correct. \
                        Output only valid JSON.",
        extra_criteria: "\
             6. Overconfidence Penalty - Does the response assert correctness \
                without sufficient justification? A wrong but confident answer \
                MUST score below 4.0.\n\
             7. Verification Quality - Did the agents genuinely test their \
                answer against constraints, or merely restate their conclusion?",
    },
    JudgePersona {
        name: "Exhaustiveness Judge",
        system_prompt: "You are an exhaustive evaluation judge who demands \
                        that EVERY possible case be explored. An answer that \
                        only tests one hypothesis and ignores alternatives is \
                        incomplete regardless of how well-written it is. \
                        Output only valid JSON.",
        extra_criteria: "\
             6. Exhaustive Case Analysis - Did the agents systematically \
                enumerate and test ALL possible configurations/solutions? \
                Mark down heavily if they only explored one path.\n\
             7. Counter-Hypothesis Testing - Did any agent explicitly try \
                to make an alternative solution work and show why it fails?",
    },
];

pub struct Orchestrator {
    pub quality_weight: f64,
    pub consistency_weight: f64,
    pub efficiency_weight: f64,
}

impl Orchestrator {
    pub fn new(config: &Config) -> Self {
        Self {
            quality_weight: config.quality_weight,
            consistency_weight: config.consistency_weight,
            efficiency_weight: config.efficiency_weight,
        }
    }

    pub async fn evaluate(
        &self,
        output: &TeamOutput,
        problem: &str,
        llm: &LlmClient,
        config: &Config,
        generation: usize,
        conclusion_history: &ConclusionHistory,
    ) -> Result<FitnessScore> {
        let (quality, judge_critique) =
            self.evaluate_quality(output, problem, llm, config, generation).await?;

        let consistency = self.evaluate_consistency(output);

        let efficiency = self.evaluate_efficiency(output, config);
        let efficiency_weight = self.decayed_efficiency_weight(generation, config.max_generations);

        let diversity_penalty = conclusion_history.penalty(output);

        let total = (self.quality_weight * quality
            + self.consistency_weight * consistency
            + efficiency_weight * efficiency
            - diversity_penalty)
            .clamp(0.0, 10.0);

        Ok(FitnessScore {
            quality,
            consistency,
            efficiency,
            diversity_penalty,
            total,
            judge_critique,
        })
    }

    /// Efficiency weight decays quadratically over generations.
    fn decayed_efficiency_weight(&self, generation: usize, max_generations: usize) -> f64 {
        if max_generations <= 1 {
            return self.efficiency_weight;
        }
        let progress = generation as f64 / (max_generations - 1) as f64;
        let decay = (1.0 - progress).powi(2);
        self.efficiency_weight * decay
    }

    async fn evaluate_quality(
        &self,
        output: &TeamOutput,
        problem: &str,
        llm: &LlmClient,
        config: &Config,
        generation: usize,
    ) -> Result<(f64, String)> {
        if output.agent_outputs.is_empty() {
            return Ok((1.0, "No agent outputs to evaluate.".into()));
        }

        let mut agent_section = String::new();
        for (i, ao) in output.agent_outputs.iter().enumerate() {
            agent_section.push_str(&format!(
                "--- Agent {} \"{}\" ({}) ---\n{}\n\n",
                i + 1,
                ao.genotype_name,
                ao.strategy,
                ao.content,
            ));
        }

        let p1 = &JUDGE_PERSONAS[generation % JUDGE_PERSONAS.len()];
        let p2 = &JUDGE_PERSONAS[(generation + 1) % JUDGE_PERSONAS.len()];

        let (r1, r2) = tokio::join!(
            self.run_judge(p1, &agent_section, problem, output.agent_outputs.len(), llm, config),
            self.run_judge(p2, &agent_section, problem, output.agent_outputs.len(), llm, config),
        );

        let (s1, c1) = r1.unwrap_or((5.0, String::new()));
        let (s2, c2) = r2.unwrap_or((5.0, String::new()));

        let score = (s1 + s2) / 2.0;

        let critique = format!(
            "[{}] {}\n\n[{}] {}",
            p1.name, c1, p2.name, c2
        );

        debug!(quality = score, persona_a = p1.name, persona_b = p2.name, "Quality evaluation (dual judge)");
        Ok((score, critique))
    }

    async fn run_judge(
        &self,
        persona: &JudgePersona,
        agent_section: &str,
        problem: &str,
        agent_count: usize,
        llm: &LlmClient,
        config: &Config,
    ) -> Result<(f64, String)> {
        let user_msg = format!(
            "You are an expert evaluator assessing the quality of AI-generated responses.\n\n\
             **Problem Statement:**\n{problem}\n\n\
             You are evaluating a team of {agent_count} AI agents. \
             Rate the overall team output quality on a scale of 1.0 to 10.0.\n\n\
             Consider:\n\
             1. Correctness - Are the responses factually accurate?\n\
             2. Completeness - Do they cover all aspects of the problem?\n\
             3. Insight - Do they go beyond surface-level analysis?\n\
             4. Actionability - Are the responses practical and useful?\n\
             5. Self-consistency - Did any agent contradict themselves or others?\n\
             {}\n\n\
             **Team Responses:**\n{agent_section}\n\
             Respond with ONLY a JSON object: {{\"score\": <number 1.0-10.0>, \"reasoning\": \"<detailed critique: what was done well, what specific logical gaps or errors exist, and what the agents should fix>\"}}",
            persona.extra_criteria,
        );

        let resp = llm
            .chat_completion(
                persona.system_prompt,
                &user_msg,
                0.1,
                1.0,
                config.max_tokens.min(512),
            )
            .await?;

        let (score, critique) = parse_quality_score(&resp.content);
        Ok((score, critique))
    }

    fn evaluate_consistency(&self, output: &TeamOutput) -> f64 {
        let outputs = &output.agent_outputs;
        if outputs.len() < 2 {
            return 10.0;
        }

        let agents: Vec<(HashSet<String>, bool)> = outputs
            .iter()
            .map(|o| {
                let words: HashSet<String> = o
                    .content
                    .to_lowercase()
                    .split_whitespace()
                    .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                    .filter(|w| !w.is_empty())
                    .collect();
                let is_red = o.strategy == crate::genotype::ReasoningStrategy::RedTeam;
                (words, is_red)
            })
            .collect();

        let mut standard_sim = 0.0_f64;
        let mut standard_pairs = 0u64;
        let mut divergence_bonus = 0.0_f64;
        let mut red_pairs = 0u64;

        for i in 0..agents.len() {
            for j in (i + 1)..agents.len() {
                let intersection = agents[i].0.intersection(&agents[j].0).count() as f64;
                let union = agents[i].0.union(&agents[j].0).count() as f64;
                let jaccard = if union > 0.0 { intersection / union } else { 0.0 };

                let either_red = agents[i].1 || agents[j].1;

                if either_red {
                    divergence_bonus += 1.0 - jaccard;
                    red_pairs += 1;
                } else {
                    standard_sim += jaccard;
                    standard_pairs += 1;
                }
            }
        }

        let base_consistency = if standard_pairs > 0 {
            standard_sim / standard_pairs as f64
        } else {
            1.0
        };

        let avg_divergence = if red_pairs > 0 {
            divergence_bonus / red_pairs as f64
        } else {
            0.0
        };

        let score = 1.0 + base_consistency * 7.0 + avg_divergence * 2.0;
        score.clamp(1.0, 10.0)
    }

    fn evaluate_efficiency(&self, output: &TeamOutput, config: &Config) -> f64 {
        let max_possible_tokens =
            config.max_tokens as f64 * output.agent_outputs.len().max(1) as f64;
        let token_ratio = output.total_tokens as f64 / max_possible_tokens;

        let score = 10.0 * (1.0 - token_ratio);
        score.clamp(1.0, 10.0)
    }
}

fn parse_quality_score(response: &str) -> (f64, String) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
        if v.is_object() {
            let score = v.get("score").and_then(|s| s.as_f64()).unwrap_or(5.0).clamp(1.0, 10.0);
            let reasoning = v.get("reasoning")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            return (score, reasoning);
        }
    }

    let trimmed = response.trim();
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            Some(&trimmed[start..=end])
        } else {
            None
        }
    } else {
        None
    };

    if let Some(json_str) = json_str {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
            let score = v.get("score").and_then(|s| s.as_f64()).unwrap_or(5.0).clamp(1.0, 10.0);
            let reasoning = v.get("reasoning")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            return (score, reasoning);
        }
    }

    let cleaned = response.replace(['`', '{', '}', '"', ','], " ");
    for token in cleaned.split_whitespace() {
        if let Ok(n) = token.parse::<f64>() {
            if (1.0..=10.0).contains(&n) {
                return (n, response.to_string());
            }
        }
    }

    (5.0, response.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_score() {
        let input = r#"{"score": 7.5, "reasoning": "Good coverage."}"#;
        let (score, reasoning) = parse_quality_score(input);
        assert!((score - 7.5).abs() < f64::EPSILON);
        assert_eq!(reasoning, "Good coverage.");
    }

    #[test]
    fn parse_bare_number() {
        assert!((parse_quality_score("8.2").0 - 8.2).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_wrapped_json() {
        let input = "```json\n{\"score\": 9.0, \"reasoning\": \"Excellent.\"}\n```";
        let (score, _) = parse_quality_score(input);
        assert!((score - 9.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_fallback() {
        assert!((parse_quality_score("no numbers here").0 - 5.0).abs() < f64::EPSILON);
    }
}
