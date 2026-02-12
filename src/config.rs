use anyhow::{bail, Result};
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Openai,
    Google,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Openai => write!(f, "OpenAI"),
            Provider::Google => write!(f, "Google Gemini"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedParams {
    pub provider: Option<Provider>,
    pub model: Option<String>,
    pub api_url: Option<String>,
    pub population: Option<usize>,
    pub team_size: Option<usize>,
    pub generations: Option<usize>,
    pub threshold: Option<f64>,
    pub mutation_rate: Option<f64>,
    pub max_tokens: Option<u32>,
    pub quality_weight: Option<f64>,
    pub consistency_weight: Option<f64>,
    pub efficiency_weight: Option<f64>,
    pub judge_model: Option<String>,
    pub judge_provider: Option<Provider>,
    pub judge_api_url: Option<String>,
}

impl SavedParams {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("emas").join("last_params.json"))
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(
                &path,
                serde_json::to_string_pretty(self).unwrap_or_default(),
            );
        }
    }

    pub fn delete() {
        if let Some(path) = Self::config_path() {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "emas",
    version,
    about = "EMAS - Evolutionary Multi-Agent System\nEvolve AI reasoning through natural selection."
)]
pub struct Cli {
    #[arg(required_unless_present = "tui")]
    pub problem: Option<String>,

    #[arg(long, default_value_t = false)]
    pub tui: bool,

    #[arg(long)]
    pub population: Option<usize>,

    #[arg(long)]
    pub team_size: Option<usize>,

    #[arg(long)]
    pub generations: Option<usize>,

    #[arg(long)]
    pub threshold: Option<f64>,

    #[arg(long)]
    pub mutation_rate: Option<f64>,

    #[arg(long, value_enum)]
    pub provider: Option<Provider>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub api_url: Option<String>,

    #[arg(long)]
    pub api_key: Option<String>,

    #[arg(long)]
    pub max_tokens: Option<u32>,

    #[arg(long)]
    pub quality_weight: Option<f64>,

    #[arg(long)]
    pub consistency_weight: Option<f64>,

    #[arg(long)]
    pub efficiency_weight: Option<f64>,

    #[arg(long)]
    pub judge_model: Option<String>,

    #[arg(long, value_enum)]
    pub judge_provider: Option<Provider>,

    #[arg(long)]
    pub judge_api_url: Option<String>,

    #[arg(long)]
    pub judge_api_key: Option<String>,

    #[arg(long, default_value_t = false)]
    pub no_save: bool,

    #[arg(long, default_value_t = false)]
    pub reset_defaults: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub api_base_url: String,
    pub api_key: String,
    pub model: String,
    pub population_size: usize,
    pub team_size: usize,
    pub max_generations: usize,
    pub fitness_threshold: f64,
    pub mutation_rate: f64,
    pub elite_count: usize,
    pub max_tokens: u32,
    pub quality_weight: f64,
    pub consistency_weight: f64,
    pub efficiency_weight: f64,
    pub judge_provider: Provider,
    pub judge_api_base_url: String,
    pub judge_api_key: String,
    pub judge_model: String,
}

impl Config {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let saved = if cli.reset_defaults {
            SavedParams::delete();
            SavedParams::default()
        } else {
            SavedParams::load()
        };

        let api_key = cli
            .api_key
            .clone()
            .or_else(|| non_empty_env("EMAS_API_KEY"))
            .or_else(|| non_empty_env("GOOGLE_API_KEY"))
            .or_else(|| non_empty_env("OPENAI_API_KEY"))
            .unwrap_or_default();

        let api_base_url = cli
            .api_url
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| non_empty_env("EMAS_API_BASE_URL"))
            .or_else(|| non_empty_env("OPENAI_API_BASE"))
            .or_else(|| saved.api_url.clone());

        let provider = if let Some(p) = cli.provider {
            p
        } else if let Some(env_prov) = non_empty_env("EMAS_PROVIDER") {
            match env_prov.to_lowercase().as_str() {
                "google" | "gemini" => Provider::Google,
                _ => Provider::Openai,
            }
        } else if let Some(p) = saved.provider {
            p
        } else if api_base_url
            .as_deref()
            .map_or(false, |u| u.contains("googleapis.com"))
        {
            Provider::Google
        } else if non_empty_env("GOOGLE_API_KEY").is_some()
            && non_empty_env("OPENAI_API_KEY").is_none()
        {
            Provider::Google
        } else {
            Provider::Openai
        };

        let api_base_url = api_base_url.unwrap_or_else(|| match provider {
            Provider::Google => {
                "https://generativelanguage.googleapis.com/v1beta".into()
            }
            Provider::Openai => "https://api.openai.com/v1".into(),
        });

        let model = cli
            .model
            .clone()
            .or_else(|| non_empty_env("EMAS_MODEL"))
            .or_else(|| match provider {
                Provider::Google => non_empty_env("GOOGLE_MODEL"),
                Provider::Openai => non_empty_env("OPENAI_MODEL"),
            })
            .or_else(|| saved.model.clone())
            .unwrap_or_else(|| match provider {
                Provider::Google => "gemini-2.0-flash-lite".into(),
                Provider::Openai => "gpt-4o-mini".into(),
            });

        let population = cli.population.or(saved.population).unwrap_or(5);
        let team_size = cli.team_size.or(saved.team_size).unwrap_or(3);
        let generations = cli.generations.or(saved.generations).unwrap_or(10);
        let threshold = cli.threshold.or(saved.threshold).unwrap_or(8.5);
        let mutation_rate = cli.mutation_rate.or(saved.mutation_rate).unwrap_or(0.3);
        let max_tokens = cli.max_tokens.or(saved.max_tokens).unwrap_or(1024);
        let quality_weight = cli.quality_weight.or(saved.quality_weight).unwrap_or(0.50);
        let consistency_weight = cli.consistency_weight.or(saved.consistency_weight).unwrap_or(0.30);
        let efficiency_weight = cli.efficiency_weight.or(saved.efficiency_weight).unwrap_or(0.20);

        if population < 2 {
            bail!("Population size must be at least 2");
        }
        if team_size < 1 {
            bail!("Team size must be at least 1");
        }
        if !(0.0..=1.0).contains(&mutation_rate) {
            bail!("Mutation rate must be between 0.0 and 1.0");
        }
        if !(1.0..=10.0).contains(&threshold) {
            bail!("Fitness threshold must be between 1.0 and 10.0");
        }
        if api_key.is_empty() {
            bail!(
                "No API key found. Set EMAS_API_KEY, GOOGLE_API_KEY, or OPENAI_API_KEY \
                 in your environment or .env file, or pass --api-key on the command line."
            );
        }

        let elite_count = ((population as f64) * 0.4).ceil() as usize;
        let elite_count = elite_count.max(1).min(population - 1);

        let judge_provider = cli
            .judge_provider
            .or(saved.judge_provider)
            .unwrap_or(provider);
        let judge_api_key = cli
            .judge_api_key
            .clone()
            .or_else(|| non_empty_env("EMAS_JUDGE_API_KEY"))
            .unwrap_or_else(|| api_key.clone());
        let judge_api_base_url = cli
            .judge_api_url
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| non_empty_env("EMAS_JUDGE_API_BASE_URL"))
            .or_else(|| saved.judge_api_url.clone())
            .unwrap_or_else(|| {
                if judge_provider == provider {
                    api_base_url.clone()
                } else {
                    match judge_provider {
                        Provider::Google => {
                            "https://generativelanguage.googleapis.com/v1beta".into()
                        }
                        Provider::Openai => "https://api.openai.com/v1".into(),
                    }
                }
            });
        let judge_model = cli
            .judge_model
            .clone()
            .or_else(|| non_empty_env("EMAS_JUDGE_MODEL"))
            .or_else(|| saved.judge_model.clone())
            .unwrap_or_else(|| model.clone());

        if !cli.no_save {
            let to_save = SavedParams {
                provider: Some(provider),
                model: Some(model.clone()),
                api_url: cli
                    .api_url
                    .clone()
                    .filter(|s| !s.trim().is_empty()),
                population: Some(population),
                team_size: Some(team_size),
                generations: Some(generations),
                threshold: Some(threshold),
                mutation_rate: Some(mutation_rate),
                max_tokens: Some(max_tokens),
                quality_weight: Some(quality_weight),
                consistency_weight: Some(consistency_weight),
                efficiency_weight: Some(efficiency_weight),
                judge_model: if judge_model != model {
                    Some(judge_model.clone())
                } else {
                    None
                },
                judge_provider: if judge_provider != provider {
                    Some(judge_provider)
                } else {
                    None
                },
                judge_api_url: cli
                    .judge_api_url
                    .clone()
                    .filter(|s| !s.trim().is_empty()),
            };
            to_save.save();
        }

        Ok(Config {
            provider,
            api_base_url,
            api_key,
            model,
            population_size: population,
            team_size,
            max_generations: generations,
            fitness_threshold: threshold,
            mutation_rate,
            elite_count,
            max_tokens,
            quality_weight,
            consistency_weight,
            efficiency_weight,
            judge_provider,
            judge_api_base_url,
            judge_api_key,
            judge_model,
        })
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}
