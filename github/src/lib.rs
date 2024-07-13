use anyhow::Result;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationTokenResult {
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone)]
pub struct GitHub {
    pub org: String,
    pub pat: String,
    client: reqwest::blocking::Client,
}

impl GitHub {
    pub fn new(org: &str, pat: &str) -> Self {
        GitHub {
            org: org.to_string(),
            pat: pat.to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn registration_token(&self) -> Result<String> {
        let registration_token_result = self
            .client
            .post(format!(
                "https://api.github.com/orgs/{}/actions/runners/registration-token",
                self.org
            ))
            .header("Authorization", format!("Bearer {}", self.pat))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "actions-runner")
            .send()?
            .json::<RegistrationTokenResult>()?;
        Ok(registration_token_result.token)
    }

    pub fn remove_runner(&self, runner_name: &str) -> Result<()> {
        self.client
            .post(format!(
                "https://api.github.com/orgs/{}/actions/runners/remove-token",
                self.org
            ))
            .header("Authorization", format!("Bearer {}", self.pat))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "actions-runner")
            .json(&serde_json::json!({
                "runner_name": runner_name
            }))
            .send()?;
        Ok(())
    }
}
