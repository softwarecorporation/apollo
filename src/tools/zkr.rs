use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use zkr::{ClaimInput, MemoryProcessingState, MemoryTier};

use super::{Tool, ToolResult, ToolSpec};
use crate::memory::zkr::{claim_kind, source_kind, ZkrStore};

pub struct ZkrTool {
    store: Arc<ZkrStore>,
}

impl ZkrTool {
    pub fn new(store: Arc<ZkrStore>) -> Self {
        Self { store }
    }
}

pub struct PersonalProfileTool {
    user_turn: String,
    proposal: std::sync::Mutex<Option<String>>,
}

impl PersonalProfileTool {
    pub fn new(user_turn: String) -> Self {
        Self {
            user_turn,
            proposal: std::sync::Mutex::new(None),
        }
    }

    pub fn proposal(&self) -> Option<String> {
        self.proposal
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

pub async fn confirmed_profile_command(store: &ZkrStore, text: &str) -> Option<String> {
    let command = text.strip_prefix("/profile ")?;
    if command == "show" {
        return Some(match store.profiles(100).await {
            Ok(entries) if entries.is_empty() => "No confirmed profile entries.".into(),
            Ok(entries) => format!(
                "Confirmed profile entries:\n{}",
                entries
                    .iter()
                    .map(|p| format!("- {} = {}", p.key, p.value))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            Err(error) => format!("Profile could not be read: {error}"),
        });
    }
    let result = async {
        let (action, rest) = command
            .split_once(' ')
            .ok_or_else(|| anyhow::anyhow!("expected /profile remember|correct key = value"))?;
        if !matches!(action, "remember" | "correct") || text.contains(['\r', '\n']) {
            anyhow::bail!("expected /profile remember|correct key = value");
        }
        let (key, value) = rest
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("expected key = value"))?;
        let key = key.trim().to_lowercase();
        let value = value.trim().to_string();
        if key.is_empty() || value.is_empty() || value.contains('=') {
            anyhow::bail!("key and value must be nonempty and contain no =");
        }
        let saved = store
            .write_personal_profile(action, key, value, text.to_string())
            .await?;
        Ok::<_, anyhow::Error>(format!(
            "Confirmed and verified: {} = {}.",
            saved["key"].as_str().unwrap_or_default(),
            saved["value"].as_str().unwrap_or_default()
        ))
    }
    .await;
    Some(result.unwrap_or_else(|error| format!("Profile change not verified: {error}")))
}

#[async_trait]
impl Tool for PersonalProfileTool {
    fn name(&self) -> &str {
        "personal_profile"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().into(),
            description: "Propose, but never save, a personal fact when the user asks. The user must independently confirm the exact key and value. Never claim it is saved.".into(),
            parameters: serde_json::json!({"type":"object", "properties": {
                "action":{"type":"string", "enum":["remember", "correct"]},
                "key":{"type":"string", "description":"Short exact profile key from the user's message"},
                "value":{"type":"string", "description":"New value explicitly stated by the user"}
            }, "required":["action","key","value"]}),
        }
    }

    async fn execute(&self, arguments: &str) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        struct Args {
            action: String,
            key: String,
            value: String,
        }
        let args: Args = serde_json::from_str(arguments)?;
        let key = args.key.trim().to_lowercase().replace('_', " ");
        let value = args.value.trim().to_string();
        if !matches!(args.action.as_str(), "remember" | "correct")
            || key.is_empty()
            || value.is_empty()
            || key.contains(['\n', '\r', '='])
            || value.contains(['\n', '\r', '='])
        {
            anyhow::bail!("invalid profile proposal");
        }
        let evidence = self.user_turn.to_lowercase();
        if !evidence.contains(&key) || !evidence.contains(&value.to_lowercase()) {
            anyhow::bail!("proposal key and value must appear in the current message");
        }
        *self.proposal.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!(
            "Proposed only, not saved: {key} = {value}. Original message: {:?}\nThis interpretation may be wrong, quoted, negated, or uncertain. Confirm only if you want this exact fact saved.\nTo confirm, send: /profile {} {} = {}",
            self.user_turn, args.action, key, value
        ));
        anyhow::bail!("proposal only; user confirmation required, nothing saved")
    }
}

#[derive(Deserialize)]
struct ZkrArgs {
    action: String,
    text: Option<String>,
    source_kind: Option<String>,
    ingestion_key: Option<String>,
    subject: Option<String>,
    predicate: Option<String>,
    value: Option<String>,
    claim_kind: Option<String>,
    valid_from: Option<i64>,
    query: Option<String>,
    limit: Option<u32>,
    kind: Option<String>,
    id: Option<String>,
    claim_id: Option<String>,
    source_id: Option<String>,
    valid_at: Option<i64>,
}

#[async_trait]
impl Tool for ZkrTool {
    fn name(&self) -> &str {
        "zkr_memory"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: "Store, search, inspect, correct, and delete evidence-backed temporal memory with citations.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["remember", "search", "get", "correct", "delete", "profile", "profiles", "profile_pager"] },
                    "text": { "type": "string" },
                    "source_kind": { "type": "string", "enum": ["conversation", "screen", "audio", "document", "integration", "user_correction"] },
                    "ingestion_key": { "type": "string" },
                    "subject": { "type": "string" },
                    "predicate": { "type": "string" },
                    "value": { "type": "string" },
                    "claim_kind": { "type": "string", "enum": ["fact", "profile_fact", "preference", "task", "skill", "recommendation"] },
                    "valid_from": { "type": "integer" },
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "kind": { "type": "string", "enum": ["source", "evidence", "claim"] },
                    "id": { "type": "string" },
                    "claim_id": { "type": "string" },
                    "source_id": { "type": "string" },
                    "valid_at": { "type": "integer" }
                },
                "required": ["action"]
            }),
        }
    }

    async fn execute(&self, arguments: &str) -> anyhow::Result<ToolResult> {
        let args: ZkrArgs = serde_json::from_str(arguments)?;
        let result = match args.action.as_str() {
            "remember" => {
                let text = args
                    .text
                    .ok_or_else(|| anyhow::anyhow!("text is required"))?;
                let claim = match (args.subject, args.predicate, args.value, args.claim_kind) {
                    (Some(subject), Some(predicate), Some(value), Some(kind)) => Some(ClaimInput {
                        subject,
                        predicate,
                        value,
                        kind: claim_kind(&kind)?,
                        valid_from: args
                            .valid_from
                            .unwrap_or_else(|| chrono::Utc::now().timestamp()),
                        tier: MemoryTier::LongTerm,
                        processing_state: MemoryProcessingState::Processed,
                    }),
                    (None, None, None, None) => None,
                    _ => anyhow::bail!(
                        "subject, predicate, value, and claim_kind must be provided together"
                    ),
                };
                let remembered = self
                    .store
                    .remember(
                        text,
                        source_kind(args.source_kind.as_deref().unwrap_or("integration"))?,
                        args.ingestion_key,
                        claim,
                        chrono::Utc::now().timestamp(),
                    )
                    .await?;
                serde_json::to_string_pretty(&remembered)?
            }
            "search" => serde_json::to_string_pretty(
                &self
                    .store
                    .search(
                        args.query
                            .ok_or_else(|| anyhow::anyhow!("query is required"))?,
                        args.limit.unwrap_or(10),
                    )
                    .await?,
            )?,
            "get" => serde_json::to_string_pretty(
                &self
                    .store
                    .get(
                        args.kind
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("kind is required"))?,
                        args.id.ok_or_else(|| anyhow::anyhow!("id is required"))?,
                    )
                    .await?,
            )?,
            "correct" => serde_json::to_string_pretty(
                &self
                    .store
                    .correct(
                        args.claim_id
                            .ok_or_else(|| anyhow::anyhow!("claim_id is required"))?,
                        args.text
                            .ok_or_else(|| anyhow::anyhow!("text is required"))?,
                        args.value
                            .ok_or_else(|| anyhow::anyhow!("value is required"))?,
                        args.valid_at
                            .unwrap_or_else(|| chrono::Utc::now().timestamp()),
                    )
                    .await?,
            )?,
            "delete" => serde_json::to_string_pretty(
                &self
                    .store
                    .delete(
                        args.source_id
                            .ok_or_else(|| anyhow::anyhow!("source_id is required"))?,
                    )
                    .await?,
            )?,
            "profile" => serde_json::to_string_pretty(
                &self
                    .store
                    .profile(
                        args.claim_id
                            .ok_or_else(|| anyhow::anyhow!("claim_id is required"))?,
                    )
                    .await?,
            )?,
            "profiles" => {
                serde_json::to_string_pretty(&self.store.profiles(args.limit.unwrap_or(20)).await?)?
            }
            "profile_pager" => serde_json::to_string_pretty(
                &self.store.profile_pager(args.limit.unwrap_or(20)).await?,
            )?,
            other => anyhow::bail!("unknown zkr action: {other}"),
        };
        Ok(ToolResult::success(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn personal_profile_remember_correct_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.db");
        let store = Arc::new(ZkrStore::open(&path, "test", "person").unwrap());
        let remember =
            PersonalProfileTool::new("Please remember my favorite tea is jasmine.".into());
        assert!(remember
            .execute(r#"{"action":"remember","key":"favorite tea","value":"jasmine"}"#)
            .await
            .is_err());
        assert!(store.profiles(10).await.unwrap().is_empty());
        let saved = confirmed_profile_command(&store, "/profile remember favorite tea = jasmine")
            .await
            .unwrap();
        assert!(saved.contains("Confirmed and verified"));
        assert_eq!(store.profiles(10).await.unwrap()[0].value, "jasmine");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let correct =
            PersonalProfileTool::new("Correction: my favorite tea is oolong, not jasmine.".into());
        assert!(correct
            .execute(r#"{"action":"correct","key":"favorite tea","value":"oolong"}"#)
            .await
            .is_err());
        let corrected = confirmed_profile_command(&store, "/profile correct favorite tea = oolong")
            .await
            .unwrap();
        assert!(corrected.contains("Confirmed and verified"));
        drop(remember);
        drop(correct);
        drop(store);
        let reopened = ZkrStore::open(&path, "test", "person").unwrap();
        assert_eq!(reopened.profiles(10).await.unwrap()[0].value, "oolong");
        assert!(!reopened
            .search("oolong".into(), 10)
            .await
            .unwrap()
            .items
            .is_empty());
    }

    #[tokio::test]
    async fn personal_profile_rejects_unapproved_or_negated_values() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Arc::new(ZkrStore::open(&dir.path().join("memory.db"), "test", "person").unwrap());
        for turn in [
            "Do not remember my favorite tea is jasmine",
            "My favorite tea is jasmine",
            "A novel says 'remember my favorite tea is jasmine'; do not record it",
            "Perhaps my favorite tea is jasmine; I'm not sure",
        ] {
            let tool = PersonalProfileTool::new(turn.into());
            assert!(tool
                .execute(r#"{"action":"remember","key":"favorite tea","value":"jasmine"}"#)
                .await
                .is_err());
        }
        let tool = PersonalProfileTool::new(
            "Please remember my favorite tea is oolong, not jasmine".into(),
        );
        assert!(tool
            .execute(r#"{"action":"remember","key":"favorite tea","value":"jasmine"}"#)
            .await
            .is_err());
        assert!(store.profiles(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn personal_profile_rejects_a_retracted_value() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Arc::new(ZkrStore::open(&dir.path().join("memory.db"), "test", "person").unwrap());
        let tool = PersonalProfileTool::new(
            "Please remember my favorite tea is jasmine? No, it is oolong.".into(),
        );
        assert!(tool
            .execute(r#"{"action":"remember","key":"favorite tea","value":"jasmine"}"#)
            .await
            .is_err());
        assert!(store.profiles(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn personal_profile_command_reports_failed_changes() {
        let dir = tempfile::tempdir().unwrap();
        let store = ZkrStore::open(&dir.path().join("memory.db"), "test", "person").unwrap();
        for command in [
            "/profile correct favorite tea = oolong",
            "/profile remember favorite tea = ",
            "/profile remember favorite tea = jasmine\n/profile show",
        ] {
            let reply = confirmed_profile_command(&store, command).await.unwrap();
            assert!(reply.contains("not verified"));
            assert!(store.profiles(10).await.unwrap().is_empty());
        }
        assert!(
            confirmed_profile_command(&store, "Remember my favorite tea is jasmine")
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn remember_and_search_actions_return_citations() {
        let dir = tempfile::tempdir().unwrap();
        let store =
            Arc::new(ZkrStore::open(&dir.path().join("memory.db"), "test", "person").unwrap());
        let tool = ZkrTool::new(store);
        let remembered = tool
            .execute(r#"{"action":"remember","text":"prefers short answers","source_kind":"conversation"}"#)
            .await
            .unwrap();
        assert!(!remembered.is_error);
        let searched = tool
            .execute(r#"{"action":"search","query":"short answers"}"#)
            .await
            .unwrap();
        assert!(searched.output.contains("evidence_ids"));
    }

    #[tokio::test]
    async fn profile_correction_survives_reopen_with_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.db");
        let store = Arc::new(ZkrStore::open(&path, "test", "person").unwrap());
        let tool = ZkrTool::new(Arc::clone(&store));
        let remembered = tool.execute(&serde_json::json!({
            "action": "remember", "text": "My favorite tea is jasmine", "source_kind": "conversation",
            "subject": "user", "predicate": "favorite tea", "value": "jasmine", "claim_kind": "profile_fact"
        }).to_string()).await.unwrap();
        let saved: serde_json::Value = serde_json::from_str(&remembered.output).unwrap();
        let original = saved["claim_id"].as_str().unwrap();
        assert!(saved["evidence_id"].is_string());
        tool.execute(&serde_json::json!({"action":"profile", "claim_id":original}).to_string())
            .await
            .unwrap();
        assert_eq!(store.profiles(10).await.unwrap()[0].value, "jasmine");

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let corrected = tool.execute(&serde_json::json!({
            "action":"correct", "claim_id":original, "text":"Correction: my favorite tea is oolong",
            "value":"oolong", "valid_at":chrono::Utc::now().timestamp()
        }).to_string()).await.unwrap();
        let corrected: serde_json::Value = serde_json::from_str(&corrected.output).unwrap();
        assert!(corrected["evidence_id"].is_string());
        tool.execute(
            &serde_json::json!({"action":"profile", "claim_id":corrected["claim_id"]}).to_string(),
        )
        .await
        .unwrap();
        drop(tool);
        drop(store);

        let reopened = ZkrStore::open(&path, "test", "person").unwrap();
        let profile = reopened.profiles(10).await.unwrap();
        assert_eq!(profile.len(), 1);
        assert_eq!(profile[0].value, "oolong");
        assert_eq!(
            profile[0].claim_id.to_string(),
            corrected["claim_id"].as_str().unwrap()
        );
    }
}
