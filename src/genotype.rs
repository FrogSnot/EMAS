use rand::seq::{IteratorRandom, SliceRandom};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasoningStrategy {
    ChainOfThought,
    Critical,
    Creative,
    Logical,
    EdgeCaseAnalysis,
    PerformanceFocus,
    DevilsAdvocate,
    FirstPrinciples,
    RedTeam,
}

impl ReasoningStrategy {
    pub fn all() -> &'static [ReasoningStrategy] {
        &[
            Self::ChainOfThought,
            Self::Critical,
            Self::Creative,
            Self::Logical,
            Self::EdgeCaseAnalysis,
            Self::PerformanceFocus,
            Self::DevilsAdvocate,
            Self::FirstPrinciples,
            Self::RedTeam,
        ]
    }

    pub fn standard() -> &'static [ReasoningStrategy] {
        &[
            Self::ChainOfThought,
            Self::Critical,
            Self::Creative,
            Self::Logical,
            Self::EdgeCaseAnalysis,
            Self::PerformanceFocus,
            Self::DevilsAdvocate,
            Self::FirstPrinciples,
        ]
    }

    pub fn random(rng: &mut impl Rng) -> Self {
        Self::standard().choose(rng).unwrap().clone()
    }

    pub fn instruction(&self) -> &'static str {
        match self {
            Self::ChainOfThought => "\
Break the problem down step-by-step. Show your reasoning at each stage. \
Number your steps clearly so the reader can follow your thought process.",
            Self::Critical => "\
Analyse the problem critically. Scrutinise every claim, look for hidden assumptions, \
logical gaps, and potential flaws. Your goal is to stress-test the reasoning.",
            Self::Creative => "\
Think laterally and creatively. Explore unconventional approaches, draw analogies \
from unrelated domains, and generate novel insights that others might miss.",
            Self::Logical => "\
Use strict formal logic. Start from axioms or clearly stated premises and build \
your argument through valid deductive or inductive steps.",
            Self::EdgeCaseAnalysis => "\
Focus relentlessly on edge cases, corner cases, and boundary conditions. Consider \
what could go wrong, what inputs might break assumptions, and where pitfalls hide.",
            Self::PerformanceFocus => "\
Prioritise performance, efficiency, and optimisation. Analyse time and space \
complexity, identify bottlenecks, and suggest benchmarks or profiling strategies.",
            Self::DevilsAdvocate => "\
Argue against the most obvious or popular solution. Find counter-examples, surface \
weaknesses, and present compelling alternative viewpoints.",
            Self::FirstPrinciples => "\
Reason from first principles. Decompose the problem into its most fundamental \
truths, discard assumptions, and reconstruct the answer from the ground up.",
            Self::RedTeam => "\
You are a Red-Team adversarial analyst. DO NOT verify or confirm the other \
agents' work. Instead:
\
1. Your PRIMARY goal is to prove there is MORE THAN ONE valid solution. \
Exhaustively enumerate every possible configuration / assignment and test \
each one against the constraints.
\
2. If you find a second valid solution the others missed, present it with \
full proof.
\
3. If you CANNOT find a second valid solution, you must explicitly show \
why every other possible configuration is impossible - walk through each \
alternative and show which constraint it violates.
\
4. NEVER say 'the agents are correct' without first completing step 3. \
Lazily agreeing is a failure of your role.",
        }
    }
}

impl fmt::Display for ReasoningStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ChainOfThought => write!(f, "Chain-of-Thought"),
            Self::Critical => write!(f, "Critical Analysis"),
            Self::Creative => write!(f, "Creative / Lateral"),
            Self::Logical => write!(f, "Strict Logic"),
            Self::EdgeCaseAnalysis => write!(f, "Edge-Case Analysis"),
            Self::PerformanceFocus => write!(f, "Performance Focus"),
            Self::DevilsAdvocate => write!(f, "Devil's Advocate"),
            Self::FirstPrinciples => write!(f, "First Principles"),
            Self::RedTeam => write!(f, "Red Team"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genotype {
    pub name: String,
    pub base_instruction: String,
    pub strategy: ReasoningStrategy,
    pub temperature: f64,
    pub top_p: f64,
    #[serde(default)]
    pub judge_feedback: Option<String>,
    #[serde(default)]
    pub is_red_team: bool,
    #[serde(default)]
    pub knowledge_hints: Vec<String>,
}

impl Genotype {
    pub fn new(name: &str, strategy: ReasoningStrategy, temperature: f64) -> Self {
        let base_instruction = match &strategy {
            ReasoningStrategy::ChainOfThought => {
                "You are a methodical analyst. Walk through problems one step at a time."
            }
            ReasoningStrategy::Critical => {
                "You are a critical analyst. Your role is to scrutinise every detail, \
                 find flaws in reasoning, and ensure the highest standard of accuracy."
            }
            ReasoningStrategy::Creative => {
                "You are a creative thinker. Your role is to explore unconventional \
                 approaches, think outside the box, and generate novel insights."
            }
            ReasoningStrategy::Logical => {
                "You are a formal logician. Your role is to construct rigorous, \
                 step-by-step arguments based on sound logical principles."
            }
            ReasoningStrategy::EdgeCaseAnalysis => {
                "You are an edge-case specialist. Your role is to find the inputs, \
                 conditions, and scenarios that break naive assumptions."
            }
            ReasoningStrategy::PerformanceFocus => {
                "You are a performance engineer. Your role is to optimise for speed, \
                 memory, and scalability while maintaining correctness."
            }
            ReasoningStrategy::DevilsAdvocate => {
                "You are a devil's advocate. Your role is to argue against the \
                 obvious solution and surface hidden weaknesses."
            }
            ReasoningStrategy::FirstPrinciples => {
                "You are a first-principles thinker. Your role is to strip away \
                 assumptions and rebuild understanding from fundamental truths."
            }
            ReasoningStrategy::RedTeam => {
                "You are a Red-Team adversarial analyst. Your role is NOT to verify \
                 other agents' answers. Instead, you must prove that alternative \
                 solutions exist, or rigorously show why every other possibility \
                 is impossible. Never lazily agree with the consensus."
            }
        };

        Self {
            name: name.to_string(),
            base_instruction: base_instruction.to_string(),
            strategy,
            temperature,
            top_p: 1.0,
            judge_feedback: None,
            is_red_team: false,
            knowledge_hints: Vec::new(),
        }
    }

    pub fn build_system_prompt(&self) -> String {
        let mut prompt = format!(
            "You are an AI reasoning agent.\n\
             Profile: {}\n\
             Reasoning Strategy: {}\n\n\
             {}\n\n\
             === Strategy Instructions ===\n\
             {}",
            self.name,
            self.strategy,
            self.base_instruction,
            self.strategy.instruction(),
        );

        if let Some(feedback) = &self.judge_feedback {
            prompt.push_str(&format!(
                "\n\n=== Feedback From Previous Generation ===\n\
                 Your predecessor received this critique from the judge:\n\
                 \"{}\"\n\
                 Your goal is to address these specific weaknesses while \
                 retaining the strengths mentioned above.",
                feedback,
            ));
        }

        if !self.knowledge_hints.is_empty() {
            prompt.push_str("\n\n=== Open Conflicts & Warnings From Prior Generations ===\n\
                             The following issues were flagged in earlier generations. \
                             These are NOT established facts - they are unresolved \
                             problems and disagreements that YOU must investigate \
                             and resolve in your analysis:\n");
            for (i, hint) in self.knowledge_hints.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, hint));
            }
            prompt.push_str(
                "Do NOT blindly trust these. Verify each one against the actual \
                 problem constraints before incorporating it into your reasoning.",
            );
        }

        prompt.push_str(
            "\n\n=== Mandatory Verification ===\n\
             BEFORE providing your final answer, you MUST perform ALL of these steps:\n\
             1. State your proposed conclusion clearly.\n\
             2. Simulate / test your conclusion: walk through EVERY premise or \n\
                constraint and confirm it holds under your answer.\n\
             3. If any check fails, revise your answer and re-verify.\n\
             4. **Counter-Hypothesis Test**: Now assume your conclusion is WRONG.\n\
                Pick the most plausible alternative answer and try to make it \n\
                work by walking through every premise / constraint again.\n\
             5. If the counter-hypothesis ALSO satisfies all constraints, your \n\
                final answer MUST declare the problem ambiguous and present \n\
                BOTH valid solutions.\n\
             6. If the counter-hypothesis fails, state exactly which constraint \n\
                it violates so the reader can see it was tested.\n\
             7. Only present your final answer once ALL checks pass.\n\
             Label this section \"## Verification\" in your response."
        );

        prompt.push_str(
            "\n\nApproach the given problem using your assigned reasoning strategy. \
             Provide a clear, well-structured response. Be thorough but concise.",
        );

        prompt
    }

    pub fn templates() -> Vec<Genotype> {
        vec![
            Genotype::new("Chain-of-Thought Analyst", ReasoningStrategy::ChainOfThought, 0.5),
            Genotype::new("Critical Analyst", ReasoningStrategy::Critical, 0.3),
            Genotype::new("Creative Thinker", ReasoningStrategy::Creative, 0.9),
            Genotype::new("Logical Reasoner", ReasoningStrategy::Logical, 0.2),
            Genotype::new("Edge-Case Specialist", ReasoningStrategy::EdgeCaseAnalysis, 0.4),
            Genotype::new("Performance Optimizer", ReasoningStrategy::PerformanceFocus, 0.3),
            Genotype::new("Devil's Advocate", ReasoningStrategy::DevilsAdvocate, 0.7),
            Genotype::new("First-Principles Thinker", ReasoningStrategy::FirstPrinciples, 0.4),
        ]
    }

    pub fn random(rng: &mut impl Rng) -> Self {
        Self::templates().into_iter().choose(rng).unwrap()
    }
}

pub const MUTATION_MODIFIERS: &[&str] = &[
    "Pay extra attention to edge cases and boundary conditions.",
    "Prioritise conciseness and clarity in your response.",
    "Consider real-world practical implications.",
    "Look for non-obvious connections and patterns.",
    "Question your own assumptions at each step.",
    "Consider the problem from multiple stakeholder perspectives.",
    "Focus on providing actionable, concrete recommendations.",
    "Emphasise quantitative analysis where possible.",
    "Think about failure modes and how to mitigate them.",
    "Draw parallels from other disciplines to strengthen your argument.",
];
