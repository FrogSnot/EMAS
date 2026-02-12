use anyhow::Result;
use colored::*;
use futures::stream::{self, StreamExt};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tokio::sync::mpsc::UnboundedSender;

use crate::config::Config;
use crate::evolution::{self, ScoredTeam};
use crate::knowledge::KnowledgeBase;
use crate::llm::LlmClient;
use crate::orchestrator::{ConclusionHistory, FitnessScore, Orchestrator};
use crate::team::{Team, TeamOutput};

#[derive(Debug, Clone)]
pub struct TeamScore {
    pub name: String,
    pub total: f64,
    pub quality: f64,
    pub consistency: f64,
    pub efficiency: f64,
}

#[derive(Debug)]
pub enum ArenaEvent {
    GenerationStarted { gen: usize, total: usize },
    GenerationComplete {
        gen: usize,
        scores: Vec<TeamScore>,
        best_name: String,
        best_score: f64,
    },
    Evolving { kept: usize, spawning: usize },
    Converged { gen: usize, score: f64 },
    Warning(String),
    SynthesisStarted,
    Completed(EvolutionResult),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct EvolutionResult {
    pub best_team: Team,
    pub best_output: TeamOutput,
    pub best_score: FitnessScore,
    pub synthesis: String,
    pub generations_run: usize,
}

pub struct Arena {
    pub config: Config,
    pub llm: LlmClient,
    pub judge_llm: LlmClient,
    pub orchestrator: Orchestrator,
}

impl Arena {
    pub fn new(config: Config) -> Self {
        let llm = LlmClient::new(
            &config.api_base_url,
            &config.api_key,
            &config.model,
            config.provider,
        );
        let judge_llm = LlmClient::new(
            &config.judge_api_base_url,
            &config.judge_api_key,
            &config.judge_model,
            config.judge_provider,
        );
        let orchestrator = Orchestrator::new(&config);
        Self {
            config,
            llm,
            judge_llm,
            orchestrator,
        }
    }

    pub async fn run(&self, problem: &str) -> Result<EvolutionResult> {
        self.print_header(problem);

        let mut rng = StdRng::from_entropy();
        let mut population = evolution::create_initial_population(&self.config, &mut rng);
        let mut knowledge = KnowledgeBase::new(15);
        let mut conclusion_history = ConclusionHistory::new();

        let mut best_ever: Option<(Team, TeamOutput, FitnessScore)> = None;
        let mut elite_cache: Vec<(Team, TeamOutput, FitnessScore)> = Vec::new();

        for gen in 0..self.config.max_generations {
            println!(
                "\n{}",
                format!("Generation {}/{}", gen + 1, self.config.max_generations)
                    .bold()
                    .cyan()
            );
            println!("{}", "-".repeat(56).dimmed());

            let mut scored: Vec<ScoredTeam> = Vec::new();

            for (team, output, score) in elite_cache.drain(..) {
                scored.push(ScoredTeam {
                    team,
                    output,
                    score,
                });
            }

            let new_teams: Vec<Team> = population
                .iter()
                .filter(|t| !scored.iter().any(|s| s.team.id == t.id))
                .cloned()
                .collect();

            if !new_teams.is_empty() {
                let team_outputs = self.execute_population(&new_teams, problem).await;
                let mut new_scored = self
                    .evaluate_population(
                        &new_teams,
                        &team_outputs,
                        problem,
                        gen,
                        &conclusion_history,
                    )
                    .await;
                scored.append(&mut new_scored);
            }

            scored.sort_by(|a, b| {
                b.score
                    .total
                    .partial_cmp(&a.score.total)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let gen_best_score = scored.first().map(|s| s.score.total).unwrap_or(0.0);
            let gen_best_name = scored
                .first()
                .map(|s| s.team.name.clone())
                .unwrap_or_default();

            for (i, st) in scored.iter().enumerate() {
                let bar = score_bar(st.score.total);
                let marker = if i == 0 {
                    ">".green().bold().to_string()
                } else {
                    " ".to_string()
                };
                println!(
                    " {} {:<18} {} {}",
                    marker,
                    st.team.name.white().bold(),
                    bar,
                    st.score.to_string().dimmed(),
                );
            }

            println!(
                "\n   {} {} ({})",
                "Best:".yellow().bold(),
                gen_best_name.green().bold(),
                format!("{:.2}", gen_best_score).green(),
            );

            for st in &scored {
                conclusion_history.record(&st.output, st.score.total);
            }

            for st in &scored {
                knowledge.extract_from_critique(&st.score.judge_critique);
            }
            let conflict_pairs: Vec<(String, &TeamOutput)> = scored
                .iter()
                .map(|st| (st.team.name.clone(), &st.output))
                .collect();
            knowledge.extract_conflicts(&conflict_pairs);
            if !knowledge.is_empty() {
                println!(
                    "   {} insights in knowledge base",
                    knowledge.len(),
                );
            }

            if best_ever
                .as_ref()
                .map_or(true, |(_, _, s)| gen_best_score > s.total)
            {
                let top = scored.remove(0);
                best_ever = Some((top.team.clone(), top.output.clone(), top.score.clone()));
                scored.insert(
                    0,
                    ScoredTeam {
                        team: top.team,
                        output: top.output,
                        score: top.score,
                    },
                );
            }

            if gen_best_score >= self.config.fitness_threshold {
                println!(
                    "\n{}",
                    format!(
                        "Converged at generation {} with score {:.2}/10",
                        gen + 1,
                        gen_best_score
                    )
                    .green()
                    .bold()
                );
                break;
            }

            if gen + 1 < self.config.max_generations {
                println!(
                    "   {} keeping top {}, spawning {} mutants...",
                    "Evolving:".magenta().bold(),
                    self.config.elite_count,
                    self.config.population_size - self.config.elite_count,
                );
                let members = evolution::next_generation(
                    &mut scored,
                    &self.config,
                    gen + 1,
                    &knowledge,
                    &mut rng,
                );
                population = Vec::with_capacity(members.len());
                for m in members {
                    population.push(m.team.clone());
                    if let Some((output, score)) = m.cached {
                        elite_cache.push((m.team, output, score));
                    }
                }
            }
        }

        let (best_team, best_output, best_score) =
            best_ever.expect("at least one generation must run");

        println!(
            "\n{}",
            "-".repeat(56).dimmed()
        );
        println!(
            "{}",
            "Synthesising final response from winning team...".bold().cyan()
        );

        let synthesis = self.synthesise(problem, &best_output).await?;

        let generations_run = best_team.generation + 1;

        Ok(EvolutionResult {
            best_team,
            best_output,
            best_score,
            synthesis,
            generations_run,
        })
    }

    pub async fn run_with_progress(
        &self,
        problem: &str,
        tx: UnboundedSender<ArenaEvent>,
    ) -> Result<EvolutionResult> {
        let mut rng = StdRng::from_entropy();
        let mut population = evolution::create_initial_population(&self.config, &mut rng);
        let mut knowledge = KnowledgeBase::new(15);
        let mut conclusion_history = ConclusionHistory::new();
        let mut best_ever: Option<(Team, TeamOutput, FitnessScore)> = None;

        let mut elite_cache: Vec<(Team, TeamOutput, FitnessScore)> = Vec::new();

        for gen in 0..self.config.max_generations {
            let _ = tx.send(ArenaEvent::GenerationStarted {
                gen: gen + 1,
                total: self.config.max_generations,
            });

            let mut scored: Vec<ScoredTeam> = Vec::new();

            for (team, output, score) in elite_cache.drain(..) {
                scored.push(ScoredTeam {
                    team,
                    output,
                    score,
                });
            }

            let new_teams: Vec<Team> = population
                .iter()
                .filter(|t| !scored.iter().any(|s| s.team.id == t.id))
                .cloned()
                .collect();

            if !new_teams.is_empty() {
                let team_outputs = self.execute_population(&new_teams, problem).await;

                for (_idx, result) in &team_outputs {
                    if let Ok(output) = result {
                        for w in &output.warnings {
                            let _ = tx.send(ArenaEvent::Warning(w.clone()));
                        }
                    }
                }

                let mut new_scored = self
                    .evaluate_population(
                        &new_teams,
                        &team_outputs,
                        problem,
                        gen,
                        &conclusion_history,
                    )
                    .await;
                scored.append(&mut new_scored);
            }

            scored.sort_by(|a, b| {
                b.score
                    .total
                    .partial_cmp(&a.score.total)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let gen_best_score = scored.first().map(|s| s.score.total).unwrap_or(0.0);
            let gen_best_name = scored
                .first()
                .map(|s| s.team.name.clone())
                .unwrap_or_default();

            let scores: Vec<TeamScore> = scored
                .iter()
                .map(|s| TeamScore {
                    name: s.team.name.clone(),
                    total: s.score.total,
                    quality: s.score.quality,
                    consistency: s.score.consistency,
                    efficiency: s.score.efficiency,
                })
                .collect();

            let _ = tx.send(ArenaEvent::GenerationComplete {
                gen: gen + 1,
                scores,
                best_name: gen_best_name.clone(),
                best_score: gen_best_score,
            });

            for st in &scored {
                conclusion_history.record(&st.output, st.score.total);
            }

            if best_ever
                .as_ref()
                .map_or(true, |(_, _, s)| gen_best_score > s.total)
            {
                let top = scored.remove(0);
                best_ever = Some((top.team.clone(), top.output.clone(), top.score.clone()));
                scored.insert(
                    0,
                    ScoredTeam {
                        team: top.team,
                        output: top.output,
                        score: top.score,
                    },
                );
            }

            if gen_best_score >= self.config.fitness_threshold {
                let _ = tx.send(ArenaEvent::Converged {
                    gen: gen + 1,
                    score: gen_best_score,
                });
                break;
            }

            for st in &scored {
                knowledge.extract_from_critique(&st.score.judge_critique);
            }
            let conflict_pairs: Vec<(String, &TeamOutput)> = scored
                .iter()
                .map(|st| (st.team.name.clone(), &st.output))
                .collect();
            knowledge.extract_conflicts(&conflict_pairs);

            if gen + 1 < self.config.max_generations {
                let _ = tx.send(ArenaEvent::Evolving {
                    kept: self.config.elite_count,
                    spawning: self.config.population_size - self.config.elite_count,
                });
                let members = evolution::next_generation(
                    &mut scored,
                    &self.config,
                    gen + 1,
                    &knowledge,
                    &mut rng,
                );
                population = Vec::with_capacity(members.len());
                for m in members {
                    population.push(m.team.clone());
                    if let Some((output, score)) = m.cached {
                        elite_cache.push((m.team, output, score));
                    }
                }
            }
        }

        let (best_team, best_output, best_score) =
            best_ever.expect("at least one generation must run");

        let _ = tx.send(ArenaEvent::SynthesisStarted);
        let synthesis = self.synthesise(problem, &best_output).await?;
        let generations_run = best_team.generation + 1;

        Ok(EvolutionResult {
            best_team,
            best_output,
            best_score,
            synthesis,
            generations_run,
        })
    }

    async fn execute_population(
        &self,
        population: &[Team],
        problem: &str,
    ) -> Vec<(usize, Result<TeamOutput>)> {
        let concurrency = population.len();

        let items: Vec<(usize, Team)> =
            population.iter().cloned().enumerate().collect();

        let results: Vec<(usize, Result<TeamOutput>)> = stream::iter(items)
            .map(|(idx, team)| async move {
                let output = team.execute(problem, &self.llm, &self.config).await;
                (idx, output)
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        results
    }

    async fn evaluate_population(
        &self,
        population: &[Team],
        team_outputs: &[(usize, Result<TeamOutput>)],
        problem: &str,
        generation: usize,
        conclusion_history: &ConclusionHistory,
    ) -> Vec<ScoredTeam> {
        let mut scored: Vec<ScoredTeam> = Vec::with_capacity(population.len());

        let mut outputs_by_idx: std::collections::HashMap<usize, TeamOutput> =
            std::collections::HashMap::new();
        for (idx, result) in team_outputs {
            if let Ok(output) = result {
                outputs_by_idx.insert(*idx, output.clone());
            }
        }

        let eval_futures: Vec<_> = population
            .iter()
            .enumerate()
            .filter_map(|(idx, team)| {
                outputs_by_idx.remove(&idx).map(|output| {
                    let team = team.clone();
                    async move {
                        let score = self
                            .orchestrator
                            .evaluate(
                                &output,
                                problem,
                                &self.judge_llm,
                                &self.config,
                                generation,
                                conclusion_history,
                            )
                            .await
                            .unwrap_or(FitnessScore {
                                quality: 1.0,
                                consistency: 1.0,
                                efficiency: 1.0,
                                diversity_penalty: 0.0,
                                total: 1.0,
                                judge_critique: String::new(),
                            });
                        ScoredTeam {
                            team,
                            output,
                            score,
                        }
                    }
                })
            })
            .collect();

        let results: Vec<ScoredTeam> =
            futures::future::join_all(eval_futures).await;
        scored.extend(results);

        scored
    }

    async fn synthesise(&self, problem: &str, output: &TeamOutput) -> Result<String> {
        let mut agent_section = String::new();
        for ao in &output.agent_outputs {
            agent_section.push_str(&format!(
                "--- {} ({}) ---\n{}\n\n",
                ao.genotype_name, ao.strategy, ao.content,
            ));
        }

        let user_msg = format!(
            "Synthesise the following expert analyses into a single, comprehensive response.\n\n\
             **Problem:** {problem}\n\n\
             **Expert Analyses:**\n{agent_section}\n\
             Provide a unified, authoritative response that incorporates the strongest \
             insights from each expert. Be thorough but concise.",
        );

        let resp = self
            .judge_llm
            .chat_completion(
                "You are a synthesis expert. Combine multiple expert analyses into one \
                 clear, comprehensive response.",
                &user_msg,
                0.3,
                1.0,
                self.config.max_tokens.saturating_mul(4),
            )
            .await?;

        Ok(resp.content)
    }

    fn print_header(&self, problem: &str) {
        println!();
        println!(
            "{}",
            "EMAS - Evolutionary Multi-Agent System"
                .bold()
                .bright_white()
        );
        println!("{}", "=".repeat(56).dimmed());
        println!();

        let truncated: String = if problem.len() > 120 {
            format!("{}...", &problem[..117])
        } else {
            problem.to_string()
        };
        println!("  {} {}", "Problem:".bold(), truncated.white());
        println!();
        println!(
            "  {} {} teams x {} agents",
            "Population:".bold(),
            self.config.population_size,
            self.config.team_size,
        );
        println!(
            "  {} {}",
            "Generations:".bold(),
            self.config.max_generations,
        );
        println!(
            "  {} {:.1}/10",
            "Threshold:".bold(),
            self.config.fitness_threshold,
        );
        println!(
            "  {} {:.0}%",
            "Mutation:".bold(),
            self.config.mutation_rate * 100.0,
        );
        println!(
            "  {} {}",
            "Provider:".bold(),
            self.config.provider,
        );
        println!(
            "  {} {}",
            "Model:".bold(),
            self.config.model,
        );
        println!(
            "  {} {}",
            "Endpoint:".bold(),
            self.config.api_base_url,
        );

        if self.config.judge_model != self.config.model
            || self.config.judge_provider != self.config.provider
        {
            println!();
            println!(
                "  {} {} ({})",
                "Judge:".bold().yellow(),
                self.config.judge_model.yellow(),
                self.config.judge_provider.to_string().dimmed(),
            );
            if self.config.judge_api_base_url != self.config.api_base_url {
                println!(
                    "  {} {}",
                    "   Endpoint:".bold(),
                    self.config.judge_api_base_url.to_string().dimmed(),
                );
            }
        }
        println!();
        println!("{}", "-".repeat(56).dimmed());
    }
}

fn score_bar(score: f64) -> String {
    let clamped = score.clamp(0.0, 10.0);
    let filled = (clamped * 2.0) as usize;
    let empty = 20_usize.saturating_sub(filled);

    let bar = format!("{}{}", "#".repeat(filled), "-".repeat(empty));

    if clamped >= 8.0 {
        bar.green().to_string()
    } else if clamped >= 5.0 {
        bar.yellow().to_string()
    } else {
        bar.red().to_string()
    }
}
