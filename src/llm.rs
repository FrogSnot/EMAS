use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::config::Provider;

pub struct LlmClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    provider: Provider,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Serialize)]
struct OaiChatRequest {
    model: String,
    messages: Vec<OaiChatMessage>,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OaiChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct OaiChatResponseBody {
    choices: Vec<OaiChoice>,
    usage: Option<OaiUsage>,
}

#[derive(Deserialize, Debug)]
struct OaiChoice {
    message: OaiChatMessage,
}

#[derive(Deserialize, Debug)]
struct OaiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    temperature: f64,
    top_p: f64,
    max_output_tokens: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiResponseBody {
    candidates: Option<Vec<GeminiCandidate>>,
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize, Debug)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    prompt_token_count: Option<u32>,
    candidates_token_count: Option<u32>,
    total_token_count: Option<u32>,
}

impl LlmClient {
    pub fn new(base_url: &str, api_key: &str, model: &str, provider: Provider) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            provider,
        }
    }

    pub async fn chat_completion(
        &self,
        system_prompt: &str,
        user_message: &str,
        temperature: f64,
        top_p: f64,
        max_tokens: u32,
    ) -> Result<LlmResponse> {
        match self.provider {
            Provider::Openai => {
                self.openai_chat(system_prompt, user_message, temperature, top_p, max_tokens)
                    .await
            }
            Provider::Google => {
                self.gemini_chat(system_prompt, user_message, temperature, top_p, max_tokens)
                    .await
            }
        }
    }

    async fn openai_chat(
        &self,
        system_prompt: &str,
        user_message: &str,
        temperature: f64,
        top_p: f64,
        max_tokens: u32,
    ) -> Result<LlmResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        let request_body = OaiChatRequest {
            model: self.model.clone(),
            messages: vec![
                OaiChatMessage {
                    role: "system".into(),
                    content: system_prompt.into(),
                },
                OaiChatMessage {
                    role: "user".into(),
                    content: user_message.into(),
                },
            ],
            temperature,
            top_p,
            max_tokens: Some(max_tokens),
        };

        debug!(url = %url, model = %self.model, provider = "openai", "Sending chat completion request");

        let http_resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !http_resp.status().is_success() {
            let status = http_resp.status();
            let body = http_resp.text().await.unwrap_or_default();
            bail!("OpenAI API error ({status}): {body}");
        }

        let body: OaiChatResponseBody = http_resp.json().await?;

        let content = body
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let (prompt_tokens, completion_tokens, total_tokens) = match body.usage {
            Some(u) => (
                u.prompt_tokens.unwrap_or(0),
                u.completion_tokens.unwrap_or(0),
                u.total_tokens.unwrap_or_else(|| {
                    u.prompt_tokens.unwrap_or(0) + u.completion_tokens.unwrap_or(0)
                }),
            ),
            None => estimate_tokens(&content),
        };

        Ok(LlmResponse {
            content,
            prompt_tokens,
            completion_tokens,
            total_tokens,
        })
    }

    async fn gemini_chat(
        &self,
        system_prompt: &str,
        user_message: &str,
        temperature: f64,
        top_p: f64,
        max_tokens: u32,
    ) -> Result<LlmResponse> {
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key,
        );

        let request_body = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".into()),
                parts: vec![GeminiPart {
                    text: user_message.into(),
                }],
            }],
            system_instruction: if system_prompt.is_empty() {
                None
            } else {
                Some(GeminiContent {
                    role: None,
                    parts: vec![GeminiPart {
                        text: system_prompt.into(),
                    }],
                })
            },
            generation_config: Some(GeminiGenerationConfig {
                temperature,
                top_p,
                max_output_tokens: max_tokens,
            }),
        };

        debug!(url = %url, model = %self.model, provider = "google", "Sending Gemini generateContent request");

        let http_resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !http_resp.status().is_success() {
            let status = http_resp.status();
            let body = http_resp.text().await.unwrap_or_default();
            bail!("Google Gemini API error ({status}): {body}");
        }

        let body: GeminiResponseBody = http_resp.json().await?;

        let content = body
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.first())
            .map(|p| p.text.clone())
            .unwrap_or_default();

        let (prompt_tokens, completion_tokens, total_tokens) =
            match body.usage_metadata {
                Some(u) => {
                    let pt = u.prompt_token_count.unwrap_or(0);
                    let ct = u.candidates_token_count.unwrap_or(0);
                    let tt = u.total_token_count.unwrap_or(pt + ct);
                    (pt, ct, tt)
                }
                None => estimate_tokens(&content),
            };

        Ok(LlmResponse {
            content,
            prompt_tokens,
            completion_tokens,
            total_tokens,
        })
    }
}

fn estimate_tokens(content: &str) -> (u32, u32, u32) {
    let est = (content.len() / 4) as u32;
    (0, est, est)
}
