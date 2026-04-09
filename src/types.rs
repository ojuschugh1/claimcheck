use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ClaimType {
    File,
    Package,
    Test,
    BugFix,
    Numeric,
}

impl fmt::Display for ClaimType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClaimType::File => write!(f, "File"),
            ClaimType::Package => write!(f, "Package"),
            ClaimType::Test => write!(f, "Test"),
            ClaimType::BugFix => write!(f, "BugFix"),
            ClaimType::Numeric => write!(f, "Numeric"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileOp {
    Create,
    Delete,
    Modify,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumericMetric {
    FilesEdited,
    Functions,
    Lines,
}

#[derive(Debug, Clone)]
pub struct Claim {
    pub claim_type: ClaimType,
    pub raw_text: String,
    pub identifier: Option<String>,
    pub file_op: Option<FileOp>,
    pub numeric_value: Option<u64>,
    pub numeric_metric: Option<NumericMetric>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerificationResult {
    Pass,
    Fail { reason: String },
    Unverifiable { reason: String },
}

#[derive(Debug, Clone)]
pub struct VerifiedClaim {
    pub claim: Claim,
    pub result: VerificationResult,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TruthScore {
    Score(u8),
    NotApplicable,
    NoClaims,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub truth_score: String,
    pub summary: ClaimSummary,
    pub claims: Vec<ClaimDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimSummary {
    pub total: usize,
    pub pass: usize,
    pub fail: usize,
    pub unverifiable: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimDetail {
    pub claim_type: String,
    pub raw_text: String,
    pub result: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssistantMessage {
    pub content: String,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ParseError: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug)]
pub struct GitError {
    pub message: String,
}

impl GitError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GitError: {}", self.message)
    }
}

impl std::error::Error for GitError {}
