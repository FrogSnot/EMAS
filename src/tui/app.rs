use anyhow::{bail, Result};

use crate::arena::{ArenaEvent, EvolutionResult, TeamScore};
use crate::config::{Cli, Config, Provider, SavedParams};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Setup,
    Running,
    Results,
}

#[derive(Debug, Clone)]
pub enum FieldKind {
    Text,
    Select {
        options: Vec<&'static str>,
        index: usize,
    },
}

#[derive(Debug, Clone)]
pub struct FormField {
    pub label: &'static str,
    pub value: String,
    pub placeholder: &'static str,
    pub kind: FieldKind,
    pub sensitive: bool,
}

impl FormField {
    fn text(label: &'static str, value: &str, placeholder: &'static str) -> Self {
        Self {
            label,
            value: value.to_string(),
            placeholder,
            kind: FieldKind::Text,
            sensitive: false,
        }
    }

    fn sensitive(label: &'static str, value: &str, placeholder: &'static str) -> Self {
        Self {
            label,
            value: value.to_string(),
            placeholder,
            kind: FieldKind::Text,
            sensitive: true,
        }
    }

    fn select(
        label: &'static str,
        options: Vec<&'static str>,
        index: usize,
    ) -> Self {
        let value = options[index].to_string();
        Self {
            label,
            value,
            placeholder: "",
            kind: FieldKind::Select { options, index },
            sensitive: false,
        }
    }

    pub fn display_value(&self) -> String {
        if self.value.is_empty() {
            self.placeholder.to_string()
        } else if self.sensitive {
            let reveal = 4.min(self.value.len());
            let hidden = self.value.len().saturating_sub(reveal);
            format!("{}{}", &self.value[..reveal], "*".repeat(hidden))
        } else {
            self.value.clone()
        }
    }

    pub fn select_next(&mut self) {
        if let FieldKind::Select { options, index } = &mut self.kind {
            *index = (*index + 1) % options.len();
            self.value = options[*index].to_string();
        }
    }

    pub fn select_prev(&mut self) {
        if let FieldKind::Select { options, index } = &mut self.kind {
            *index = if *index == 0 {
                options.len() - 1
            } else {
                *index - 1
            };
            self.value = options[*index].to_string();
        }
    }
}

pub const F_PROBLEM: usize = 0;
pub const F_PROVIDER: usize = 1;
pub const F_MODEL: usize = 2;
pub const F_API_URL: usize = 3;
pub const F_API_KEY: usize = 4;
pub const F_JUDGE_PROVIDER: usize = 5;
pub const F_JUDGE_MODEL: usize = 6;
pub const F_POPULATION: usize = 7;
pub const F_TEAM_SIZE: usize = 8;
pub const F_GENERATIONS: usize = 9;
pub const F_THRESHOLD: usize = 10;
pub const F_MUTATION: usize = 11;
pub const F_QUALITY_W: usize = 12;
pub const F_CONSISTENCY_W: usize = 13;
pub const F_EFFICIENCY_W: usize = 14;

pub const FIELD_COUNT: usize = 15;

pub struct App {
    pub screen: Screen,
    pub fields: Vec<FormField>,
    pub selected_field: usize,
    pub editing: bool,
    pub cursor: usize,
    pub error_message: Option<String>,

    pub generation: usize,
    pub max_generations: usize,
    pub team_scores: Vec<TeamScore>,
    pub best_name: String,
    pub best_score: f64,
    pub logs: Vec<String>,
    pub status: String,

    pub result: Option<EvolutionResult>,
    pub scroll_offset: u16,

    pub should_quit: bool,
    pub start_requested: bool,
}

impl App {
    pub fn new(cli: &Cli) -> Self {
        let saved = SavedParams::load();

        let provider_idx = match cli.provider.or(saved.provider) {
            Some(Provider::Google) => 1,
            _ => 0,
        };
        let judge_provider_idx = match cli.judge_provider.or(saved.judge_provider) {
            None => 0,           // "Auto"
            Some(Provider::Openai) => 1,
            Some(Provider::Google) => 2,
        };

        let population = cli.population.or(saved.population).unwrap_or(5);
        let team_size = cli.team_size.or(saved.team_size).unwrap_or(3);
        let generations = cli.generations.or(saved.generations).unwrap_or(10);
        let threshold = cli.threshold.or(saved.threshold).unwrap_or(8.5);
        let mutation_rate = cli.mutation_rate.or(saved.mutation_rate).unwrap_or(0.3);
        let quality_weight = cli.quality_weight.or(saved.quality_weight).unwrap_or(0.50);
        let consistency_weight = cli.consistency_weight.or(saved.consistency_weight).unwrap_or(0.30);
        let efficiency_weight = cli.efficiency_weight.or(saved.efficiency_weight).unwrap_or(0.20);

        let model_str = cli.model.as_deref()
            .or(saved.model.as_deref())
            .unwrap_or("");
        let api_url_str = cli.api_url.as_deref()
            .or(saved.api_url.as_deref())
            .unwrap_or("");
        let judge_model_str = cli.judge_model.as_deref()
            .or(saved.judge_model.as_deref())
            .unwrap_or("");

        let fields = vec![
            FormField::text(
                "Problem",
                cli.problem.as_deref().unwrap_or(""),
                "Describe the problem to solve...",
            ),
            FormField::select("Provider", vec!["openai", "google"], provider_idx),
            FormField::text(
                "Model",
                model_str,
                "(auto)",
            ),
            FormField::text(
                "API URL",
                api_url_str,
                "(auto)",
            ),
            FormField::sensitive(
                "API Key",
                cli.api_key.as_deref().unwrap_or(""),
                "$EMAS_API_KEY / $GOOGLE_API_KEY / $OPENAI_API_KEY",
            ),
            FormField::select(
                "Judge Provider",
                vec!["auto", "openai", "google"],
                judge_provider_idx,
            ),
            FormField::text(
                "Judge Model",
                judge_model_str,
                "(same as agent)",
            ),
            FormField::text("Population", &population.to_string(), "5"),
            FormField::text("Team Size", &team_size.to_string(), "3"),
            FormField::text("Generations", &generations.to_string(), "10"),
            FormField::text("Threshold", &format!("{:.1}", threshold), "8.5"),
            FormField::text("Mutation Rate", &format!("{:.2}", mutation_rate), "0.30"),
            FormField::text("Quality Weight", &format!("{:.2}", quality_weight), "0.50"),
            FormField::text("Consistency Weight", &format!("{:.2}", consistency_weight), "0.30"),
            FormField::text("Efficiency Weight", &format!("{:.2}", efficiency_weight), "0.20"),
        ];

        Self {
            screen: Screen::Setup,
            fields,
            selected_field: F_PROBLEM,
            editing: false,
            cursor: 0,
            error_message: None,

            generation: 0,
            max_generations: 0,
            team_scores: Vec::new(),
            best_name: String::new(),
            best_score: 0.0,
            logs: Vec::new(),
            status: "Waiting...".into(),

            result: None,
            scroll_offset: 0,

            should_quit: false,
            start_requested: false,
        }
    }

    pub fn problem_text(&self) -> &str {
        &self.fields[F_PROBLEM].value
    }

    fn field_val(&self, idx: usize) -> &str {
        let v = &self.fields[idx].value;
        if v.is_empty() {
            self.fields[idx].placeholder
        } else {
            v
        }
    }

    fn field_f64(&self, idx: usize) -> Result<f64> {
        let v = self.field_val(idx);
        v.parse::<f64>()
            .map_err(|_| anyhow::anyhow!("'{}' is not a valid number for {}", v, self.fields[idx].label))
    }

    fn field_usize(&self, idx: usize) -> Result<usize> {
        let v = self.field_val(idx);
        v.parse::<usize>()
            .map_err(|_| anyhow::anyhow!("'{}' is not a valid integer for {}", v, self.fields[idx].label))
    }

    pub fn build_config(&self) -> Result<(Config, String)> {
        let problem = self.problem_text().to_string();
        if problem.trim().is_empty() {
            bail!("Problem cannot be empty");
        }

        let provider_str = self.field_val(F_PROVIDER);
        let provider = match provider_str {
            "google" => Some(Provider::Google),
            _ => Some(Provider::Openai),
        };

        let judge_provider_str = self.field_val(F_JUDGE_PROVIDER);
        let judge_provider = match judge_provider_str {
            "openai" => Some(Provider::Openai),
            "google" => Some(Provider::Google),
            _ => None,
        };

        let model = {
            let v = &self.fields[F_MODEL].value;
            if v.is_empty() { None } else { Some(v.clone()) }
        };
        let api_url = {
            let v = &self.fields[F_API_URL].value;
            if v.is_empty() { None } else { Some(v.clone()) }
        };
        let api_key = {
            let v = &self.fields[F_API_KEY].value;
            if v.is_empty() { None } else { Some(v.clone()) }
        };
        let judge_model = {
            let v = &self.fields[F_JUDGE_MODEL].value;
            if v.is_empty() { None } else { Some(v.clone()) }
        };

        let cli = Cli {
            problem: Some(problem.clone()),
            tui: true,
            population: Some(self.field_usize(F_POPULATION)?),
            team_size: Some(self.field_usize(F_TEAM_SIZE)?),
            generations: Some(self.field_usize(F_GENERATIONS)?),
            threshold: Some(self.field_f64(F_THRESHOLD)?),
            mutation_rate: Some(self.field_f64(F_MUTATION)?),
            provider,
            model,
            api_url,
            api_key,
            max_tokens: None,
            quality_weight: Some(self.field_f64(F_QUALITY_W)?),
            consistency_weight: Some(self.field_f64(F_CONSISTENCY_W)?),
            efficiency_weight: Some(self.field_f64(F_EFFICIENCY_W)?),
            judge_model,
            judge_provider,
            judge_api_url: None,
            judge_api_key: None,
            no_save: false,
            reset_defaults: false,
        };

        let config = Config::from_cli(&cli)?;
        Ok((config, problem))
    }

    pub fn handle_arena_event(&mut self, event: ArenaEvent) {
        let now = chrono_now();
        match event {
            ArenaEvent::GenerationStarted { gen, total } => {
                self.generation = gen;
                self.max_generations = total;
                self.status = format!("Generation {}/{}  |  executing teams...", gen, total);
                self.logs
                    .push(format!("[{}] Generation {}/{} started", now, gen, total));
            }
            ArenaEvent::GenerationComplete {
                gen,
                scores,
                best_name,
                best_score,
            } => {
                self.team_scores = scores;
                self.best_name = best_name.clone();
                self.best_score = best_score;
                self.status = format!(
                    "Generation {}/{}  |  best: {} ({:.2})",
                    gen, self.max_generations, best_name, best_score
                );
                self.logs.push(format!(
                    "[{}] Gen {} evaluated  |  Best: {} = {:.2}",
                    now, gen, best_name, best_score
                ));
            }
            ArenaEvent::Evolving { kept, spawning } => {
                self.logs.push(format!(
                    "[{}] Evolving: {} elites kept, {} children spawned",
                    now, kept, spawning
                ));
            }
            ArenaEvent::Converged { gen, score } => {
                self.status = format!("Converged at generation {} ({:.2}/10)", gen, score);
                self.logs.push(format!(
                    "[{}] Converged at gen {} with score {:.2}",
                    now, gen, score
                ));
            }
            ArenaEvent::SynthesisStarted => {
                self.status = "Synthesising final response...".into();
                self.logs
                    .push(format!("[{}] Synthesising final response...", now));
            }
            ArenaEvent::Warning(msg) => {
                self.logs.push(format!("[{}] Warning: {}", now, msg));
            }
            ArenaEvent::Completed(result) => {
                self.result = Some(result);
                self.screen = Screen::Results;
                self.scroll_offset = 0;
                self.status = "Done".into();
                self.logs.push(format!("[{}] Evolution complete!", now));
            }
            ArenaEvent::Error(msg) => {
                self.status = format!("Error: {}", msg);
                self.logs.push(format!("[{}] Error: {}", now, msg));
                self.error_message = Some(msg);
                self.screen = Screen::Setup;
            }
        }
    }

    pub fn reset_for_new_run(&mut self) {
        self.screen = Screen::Setup;
        self.generation = 0;
        self.max_generations = 0;
        self.team_scores.clear();
        self.best_name.clear();
        self.best_score = 0.0;
        self.logs.clear();
        self.status = "Waiting...".into();
        self.result = None;
        self.scroll_offset = 0;
        self.error_message = None;
    }

    pub fn insert_char(&mut self, c: char) {
        let field = &mut self.fields[self.selected_field];
        if self.cursor > field.value.len() {
            self.cursor = field.value.len();
        }
        field.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_char_back(&mut self) {
        let field = &mut self.fields[self.selected_field];
        if self.cursor > 0 && !field.value.is_empty() {
            let prev = field.value[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            field.value.remove(prev);
            self.cursor = prev;
        }
    }

    pub fn delete_char_forward(&mut self) {
        let field = &mut self.fields[self.selected_field];
        if self.cursor < field.value.len() {
            field.value.remove(self.cursor);
        }
    }

    pub fn move_cursor_left(&mut self) {
        let field = &self.fields[self.selected_field];
        if self.cursor > 0 {
            self.cursor = field.value[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_cursor_right(&mut self) {
        let field = &self.fields[self.selected_field];
        if self.cursor < field.value.len() {
            self.cursor += field.value[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor = self.fields[self.selected_field].value.len();
    }
}

fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}
