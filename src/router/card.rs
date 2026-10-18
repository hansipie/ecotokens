//! The task card: what Jev says about a message beyond its size.
//!
//! One request answers six questions; the four added here (`nature`, `risk`,
//! `ambiguity`, `verifiability`) complete the card the policy rules read.
//! Choice answers carry a confidence; the card decides how far each can be
//! trusted ([`TaskCard::fallback`], FR-005).

use serde::Serialize;

use super::Size;
use crate::jev::Answers;

pub const NATURE_QUESTION: &str = "nature";
pub const RISK_QUESTION: &str = "risk";
pub const AMBIGUITY_QUESTION: &str = "ambiguity";
pub const VERIFIABILITY_QUESTION: &str = "verifiability";

/// What kind of work the message asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Nature {
    Mechanical,
    Implementation,
    Bug,
    Design,
    Ui,
    Research,
    Sensitive,
}

impl Nature {
    pub const ALL: [Nature; 7] = [
        Nature::Mechanical,
        Nature::Implementation,
        Nature::Bug,
        Nature::Design,
        Nature::Ui,
        Nature::Research,
        Nature::Sensitive,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Nature::Mechanical => "mechanical",
            Nature::Implementation => "implementation",
            Nature::Bug => "bug",
            Nature::Design => "design",
            Nature::Ui => "ui",
            Nature::Research => "research",
            Nature::Sensitive => "sensitive",
        }
    }

    pub fn parse(s: &str) -> Option<Nature> {
        Nature::ALL.into_iter().find(|n| n.as_str() == s)
    }

    /// What the answer means, sent to Jev as the choice criterion.
    pub fn criteria(self) -> &'static str {
        match self {
            Nature::Mechanical => {
                "a repetitive, rule-based change (rename, reformat, move, bulk edit) \
                 that a script could do and a test can check"
            }
            Nature::Implementation => "building or changing a feature from a clear description",
            Nature::Bug => "finding and fixing a defect, or explaining an error",
            Nature::Design => "choosing an approach, architecture or plan before anything is built",
            Nature::Ui => "visual or interface work whose result is judged by looking at it",
            Nature::Research => "finding, comparing and citing information from outside sources",
            Nature::Sensitive => {
                "work touching money, credentials, personal data, security, \
                 legal matters or production systems"
            }
        }
    }
}

/// How much harm a wrong result could do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Reversible,
    Sensitive,
    Destructive,
}

impl Risk {
    pub const ALL: [Risk; 3] = [Risk::Reversible, Risk::Sensitive, Risk::Destructive];

    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Reversible => "reversible",
            Risk::Sensitive => "sensitive",
            Risk::Destructive => "destructive",
        }
    }

    pub fn parse(s: &str) -> Option<Risk> {
        Risk::ALL.into_iter().find(|r| r.as_str() == s)
    }

    pub fn criteria(self) -> &'static str {
        match self {
            Risk::Reversible => {
                "a wrong result is easy to notice and undo (files under version control)"
            }
            Risk::Sensitive => {
                "a wrong result could leak or corrupt secrets, money, personal data or access"
            }
            Risk::Destructive => {
                "the job deletes, overwrites, deploys or sends something that cannot be taken back"
            }
        }
    }
}

/// A choice answer and how sure Jev was.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Graded<T> {
    pub value: T,
    pub confidence: f64,
}

/// What the card says; every part except the size may be missing.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskCard {
    pub size: Size,
    pub size_confidence: f64,
    pub nature: Option<Graded<Nature>>,
    pub risk: Option<Graded<Risk>>,
    /// Probability that decisions are missing.
    pub ambiguity: Option<f64>,
    /// Probability that the result can be checked by tests, a build, a
    /// browser or a visible state.
    pub verifiability: Option<f64>,
}

/// What to do with a card, from the least to the most trusting (FR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    /// The size is unreliable: the main session keeps the message.
    MainSession,
    /// The risk is unreliable: size-only route, one tier higher.
    Raise,
    /// Only the nature is unreliable: rules that need it do not match.
    SizeOnly,
    /// Everything is reliable: the rules decide.
    Rules,
}

impl TaskCard {
    /// Reads the card from Jev's answers. `None` when the size is missing or
    /// unknown (the whole decision is then an error). Unknown nature or risk
    /// values count as missing.
    pub fn from_answers(answers: &Answers) -> Option<TaskCard> {
        let size_answer = answers.choice(super::SIZE_QUESTION)?;
        let size = Size::parse(&size_answer.choice)?;
        let graded = |id: &str| answers.choice(id);
        Some(TaskCard {
            size,
            size_confidence: size_answer.confidence,
            nature: graded(NATURE_QUESTION).and_then(|c| {
                Some(Graded {
                    value: Nature::parse(&c.choice)?,
                    confidence: c.confidence,
                })
            }),
            risk: graded(RISK_QUESTION).and_then(|c| {
                Some(Graded {
                    value: Risk::parse(&c.choice)?,
                    confidence: c.confidence,
                })
            }),
            ambiguity: answers.noul(AMBIGUITY_QUESTION),
            verifiability: answers.noul(VERIFIABILITY_QUESTION),
        })
    }

    pub fn size_reliable(&self, min: f64) -> bool {
        self.size_confidence >= min
    }

    pub fn nature_reliable(&self, min: f64) -> bool {
        self.nature.is_some_and(|n| n.confidence >= min)
    }

    pub fn risk_reliable(&self, min: f64) -> bool {
        self.risk.is_some_and(|r| r.confidence >= min)
    }

    /// FR-005 precedence: size, then risk, then nature.
    pub fn fallback(&self, min: f64) -> Fallback {
        if !self.size_reliable(min) {
            Fallback::MainSession
        } else if !self.risk_reliable(min) {
            Fallback::Raise
        } else if !self.nature_reliable(min) {
            Fallback::SizeOnly
        } else {
            Fallback::Rules
        }
    }
}
