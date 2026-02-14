use rand::seq::SliceRandom;
use rand::Rng;
use uuid::Uuid;

use crate::agent::Agent;
use crate::config::Config;
use crate::genotype::{Genotype, ReasoningStrategy, MUTATION_MODIFIERS};
use crate::knowledge::KnowledgeBase;
use crate::orchestrator::FitnessScore;
use crate::team::{Team, TeamOutput};

const GREEK: &[&str] = &[
    "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta",
    "Iota", "Kappa", "Lambda", "Mu", "Nu", "Xi", "Omicron", "Pi", "Rho",
    "Sigma", "Tau", "Upsilon", "Phi", "Chi", "Psi", "Omega",
];

fn team_name(index: usize, generation: usize) -> String {
    let letter = GREEK[index % GREEK.len()];
    if generation == 0 {
        letter.to_string()
    } else {
        format!("{letter}-G{generation}")
    }
}

pub fn create_initial_population(config: &Config, rng: &mut impl Rng) -> Vec<Team> {
    let templates = Genotype::templates();
    let mut teams = Vec::with_capacity(config.population_size);

    for team_idx in 0..config.population_size {
        let mut indices: Vec<usize> = (0..templates.len()).collect();
        indices.shuffle(rng);

        let standard_count = if config.team_size > 1 {
            config.team_size - 1
        } else {
            config.team_size
        };

        let mut agents: Vec<Agent> = (0..standard_count)
            .map(|i| {
                let genotype = templates[indices[i % indices.len()]].clone();
                Agent::new(genotype)
            })
            .collect();

        if config.team_size > 1 {
            let mut red = Genotype::new("Red Team Analyst", ReasoningStrategy::RedTeam, 0.5);
            red.is_red_team = true;
            agents.push(Agent::new(red));
        }

        teams.push(Team {
            id: Uuid::new_v4(),
            name: team_name(team_idx, 0),
            agents,
            generation: 0,
        });
    }

    teams
}

pub struct ScoredTeam {
    pub team: Team,
    pub output: TeamOutput,
    pub score: FitnessScore,
}

pub struct PopulationMember {
    pub team: Team,
    pub cached: Option<(TeamOutput, FitnessScore)>,
}

pub fn crossover(
    parent_a: &Team,
    parent_b: &Team,
    team_size: usize,
    generation: usize,
    child_index: usize,
    rng: &mut impl Rng,
) -> Team {
    let standard_count = if team_size > 1 { team_size - 1 } else { team_size };
    let mut agents = Vec::with_capacity(team_size);

    for i in 0..standard_count {
        let donor_a = parent_a.agents.get(i % parent_a.agents.len());
        let donor_b = parent_b.agents.get(i % parent_b.agents.len());

        let genotype = match (donor_a, donor_b) {
            (Some(a), Some(b)) => {
                let ga = if a.genotype.is_red_team { &b.genotype } else { &a.genotype };
                let gb = if b.genotype.is_red_team { &a.genotype } else { &b.genotype };
                let base = if rng.gen_bool(0.5) {
                    ga.clone()
                } else {
                    gb.clone()
                };
                let blended_temp = (ga.temperature + gb.temperature) / 2.0;
                Genotype {
                    temperature: blended_temp,
                    is_red_team: false,
                    ..base
                }
            }
            (Some(a), None) => a.genotype.clone(),
            (None, Some(b)) => b.genotype.clone(),
            (None, None) => Genotype::random(rng),
        };

        agents.push(Agent::new(genotype));
    }

    if team_size > 1 {
        let mut red = Genotype::new("Red Team Analyst", ReasoningStrategy::RedTeam, 0.5);
        red.is_red_team = true;
        agents.push(Agent::new(red));
    }

    Team {
        id: Uuid::new_v4(),
        name: team_name(child_index, generation),
        agents,
        generation,
    }
}

pub fn mutate(
    team: &mut Team,
    mutation_rate: f64,
    judge_feedback: Option<&str>,
    knowledge: &KnowledgeBase,
    rng: &mut impl Rng,
) {
    for agent in &mut team.agents {
        let hints: Vec<String> = knowledge.hints().to_vec();
        if !hints.is_empty() {
            agent.genotype.knowledge_hints = hints;
        }

        if let Some(fb) = judge_feedback {
            if !fb.is_empty() {
                agent.genotype.judge_feedback = Some(fb.to_string());
            }
        }

        if !rng.gen_bool(mutation_rate.clamp(0.0, 1.0)) {
            continue;
        }

        let g = &mut agent.genotype;

        let delta: f64 = rng.gen_range(-0.15..=0.15);
        g.temperature = (g.temperature + delta).clamp(0.0, 2.0);

        if !g.is_red_team && rng.gen_bool(0.25) {
            let new_strategy = ReasoningStrategy::random(rng);
            if new_strategy != g.strategy {
                g.strategy = new_strategy;
                g.name = format!("{} (mutated)", g.strategy);
            }
        }

        if rng.gen_bool(0.20) {
            if let Some(modifier) = MUTATION_MODIFIERS.choose(rng) {
                if !g.base_instruction.contains(modifier) {
                    g.base_instruction.push(' ');
                    g.base_instruction.push_str(modifier);
                }
            }
        }
    }
}

pub fn next_generation(
    scored: &mut Vec<ScoredTeam>,
    config: &Config,
    generation: usize,
    knowledge: &KnowledgeBase,
    rng: &mut impl Rng,
) -> Vec<PopulationMember> {
    let best_feedback: Option<String> = scored
        .first()
        .map(|s| s.score.judge_critique.clone())
        .filter(|s| !s.is_empty());

    scored.sort_by(|a, b| {
        b.score
            .total
            .partial_cmp(&a.score.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let elite_scored: Vec<ScoredTeam> = scored.drain(..config.elite_count.min(scored.len())).collect();
    let elite_teams: Vec<Team> = elite_scored.iter().map(|s| s.team.clone()).collect();

    let mut next_pop: Vec<PopulationMember> = Vec::with_capacity(config.population_size);

    for (i, st) in elite_scored.into_iter().enumerate() {
        let mut elite = st.team;
        elite.generation = generation;
        elite.name = team_name(i, generation);
        for agent in &mut elite.agents {
            let hints: Vec<String> = knowledge.hints().to_vec();
            if !hints.is_empty() {
                agent.genotype.knowledge_hints = hints;
            }
        }
        next_pop.push(PopulationMember {
            team: elite,
            cached: Some((st.output, st.score)),
        });
    }

    let mut child_idx = next_pop.len();
    while next_pop.len() < config.population_size {
        let pa = &elite_teams[rng.gen_range(0..elite_teams.len())];
        let pb = &elite_teams[rng.gen_range(0..elite_teams.len())];

        let mut child = crossover(pa, pb, config.team_size, generation, child_idx, rng);
        mutate(
            &mut child,
            config.mutation_rate,
            best_feedback.as_deref(),
            knowledge,
            rng,
        );
        next_pop.push(PopulationMember {
            team: child,
            cached: None,
        });
        child_idx += 1;
    }

    next_pop
}
