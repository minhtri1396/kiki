//! Choosing the Epic issue type and building the `fields` of the create-epic request from Jira's create metadata.

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use kiki_util::Ctx;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::login::Login;
use super::AtlassianApi;
use crate::urlenc::path_escape;
use crate::wire::{clip_bytes, decode, nd, one_line, value_into};

/// The longest description text sent, in bytes.
const MAX_DESCRIPTION: usize = 4000;

impl AtlassianApi {
    /// Fields for the epic, shaped by what the project's Epic issue type asks for.
    pub(super) fn epic_fields(
        &self,
        ctx: &Ctx,
        login: &Login,
        project_key: &str,
        summary: &str,
        description: &str,
    ) -> Result<Map<String, Value>> {
        let origin = &login.origin;
        let types_url = format!(
            "{origin}/rest/api/3/issue/createmeta/{}/issuetypes",
            path_escape(project_key)
        );
        let body = self.read_ok(ctx, &types_url, login, "Jira epic")?;
        let Some(issue_type) = pick_epic_type(&body)? else {
            return Err(anyhow!("Jira project {project_key} has no Epic issue type"));
        };
        let fields_url = format!("{types_url}/{}", path_escape(&issue_type.id));
        let fields_body = self.read_ok(ctx, &fields_url, login, "Jira epic")?;
        let specs = decode_fields(&fields_body)?;
        Ok(epic_field_map(
            project_key,
            &issue_type.id,
            summary,
            description,
            &specs,
        ))
    }
}

/// The minimal fields that work on most projects, used when Jira's metadata cannot be read.
pub(super) fn fallback_epic_fields(project_key: &str, summary: &str, description: &str) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("project".into(), json!({ "key": project_key }));
    fields.insert("summary".into(), json!(summary));
    fields.insert("issuetype".into(), json!({ "name": "Epic" }));
    if !description.is_empty() {
        fields.insert("description".into(), adf_doc(description));
    }
    fields
}

#[derive(Deserialize, Default, Clone, Debug)]
pub(super) struct IssueType {
    #[serde(default, deserialize_with = "nd")]
    id: String,
    #[serde(default, deserialize_with = "nd")]
    name: String,
    #[serde(default, rename = "untranslatedName", deserialize_with = "nd")]
    untranslated: String,
    #[serde(default, rename = "hierarchyLevel", deserialize_with = "nd")]
    hierarchy: i64,
}

/// An issue type named Epic, or else the first one at hierarchy level 1.
pub(super) fn pick_epic_type(body: &[u8]) -> Result<Option<IssueType>> {
    let list = decode_issue_types(body)?;
    let mut level: Option<IssueType> = None;
    for mut item in list {
        item.id = item.id.trim().to_string();
        if item.id.is_empty() {
            continue;
        }
        if item.name.trim().to_lowercase() == "epic" || item.untranslated.trim().to_lowercase() == "epic" {
            return Ok(Some(item));
        }
        if item.hierarchy == 1 && level.is_none() {
            level = Some(item);
        }
    }
    Ok(level)
}

fn decode_issue_types(body: &[u8]) -> Result<Vec<IssueType>> {
    #[derive(Deserialize, Default)]
    struct Wrapped {
        #[serde(default, rename = "issueTypes", deserialize_with = "nd")]
        issue_types: Vec<IssueType>,
        #[serde(default, deserialize_with = "nd")]
        values: Vec<IssueType>,
    }
    let wrapped: Wrapped = decode(body).map_err(|e| anyhow!("Jira epic: {}", one_line(&e)))?;
    if !wrapped.issue_types.is_empty() {
        return Ok(wrapped.issue_types);
    }
    if !wrapped.values.is_empty() {
        return Ok(wrapped.values);
    }
    // A bare array never gets here: the object decode above already rejected it.
    let list: Vec<IssueType> = serde_json::from_slice(body).unwrap_or_default();
    if list.is_empty() {
        return Err(anyhow!("Jira did not list issue types"));
    }
    Ok(list)
}

struct FieldSpec {
    field_id: String,
    name: String,
    required: bool,
    custom: String,
    system: String,
}

#[derive(Deserialize, Default)]
struct RawSchema {
    #[serde(default, deserialize_with = "nd")]
    system: String,
    #[serde(default, deserialize_with = "nd")]
    custom: String,
}

#[derive(Deserialize, Default)]
struct RawField {
    #[serde(default, rename = "fieldId", deserialize_with = "nd")]
    field_id: String,
    #[serde(default, deserialize_with = "nd")]
    name: String,
    #[serde(default, deserialize_with = "nd")]
    required: bool,
    #[serde(default, deserialize_with = "nd")]
    schema: RawSchema,
}

/// Reads the field list, which Jira sends either as an array or as an object keyed by field id.
fn decode_fields(body: &[u8]) -> Result<Vec<FieldSpec>> {
    #[derive(Deserialize, Default)]
    struct Wrapped {
        #[serde(default)]
        fields: Option<Value>,
    }
    let wrapped: Wrapped = decode(body).map_err(|e| anyhow!("Jira epic: {}", one_line(&e)))?;
    let list: Vec<RawField> = match wrapped.fields {
        None | Some(Value::Null) => return Err(anyhow!("Jira did not list epic fields")),
        Some(Value::Array(items)) => items.into_iter().map(raw_field).collect::<Result<Vec<_>>>()?,
        Some(Value::Object(object)) => fields_by_id(object)?,
        Some(_) => {
            return Err(anyhow!(
                "Jira epic: {}",
                one_line("json: cannot unmarshal into Go value of type map[string]mcp.rawField")
            ));
        }
    };
    Ok(list
        .into_iter()
        .map(|field| FieldSpec {
            field_id: field.field_id.trim().to_string(),
            name: field.name.trim().to_string(),
            required: field.required,
            custom: field.schema.custom,
            system: field.schema.system,
        })
        .collect())
}

fn raw_field(value: Value) -> Result<RawField> {
    value_into::<RawField>(value).map_err(|e| anyhow!("Jira epic: {}", one_line(&e)))
}

/// Fields from an object keyed by id, in id order; a field without its own `fieldId` takes the key.
fn fields_by_id(object: Map<String, Value>) -> Result<Vec<RawField>> {
    let sorted: BTreeMap<String, Value> = object.into_iter().collect();
    let mut list = Vec::with_capacity(sorted.len());
    for (id, value) in sorted {
        let mut field = raw_field(value)?;
        if field.field_id.trim().is_empty() {
            field.field_id = id;
        }
        list.push(field);
    }
    Ok(list)
}

fn epic_field_map(
    project_key: &str,
    type_id: &str,
    summary: &str,
    description: &str,
    specs: &[FieldSpec],
) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("project".into(), json!({ "key": project_key }));
    fields.insert("issuetype".into(), json!({ "id": type_id }));
    fields.insert("summary".into(), json!(summary));
    for spec in specs {
        if matches!(
            spec.field_id.as_str(),
            "" | "project" | "issuetype" | "summary" | "reporter" | "assignee"
        ) {
            continue;
        }
        if epic_name_field(spec) {
            fields.insert(spec.field_id.clone(), json!(summary));
            continue;
        }
        if spec.field_id == "description" || spec.system == "description" {
            let mut text = description;
            if text.is_empty() && spec.required {
                text = summary;
            }
            if !text.is_empty() {
                fields.insert("description".into(), adf_doc(text));
            }
        }
    }
    if !description.is_empty() && !fields.contains_key("description") {
        fields.insert("description".into(), adf_doc(description));
    }
    fields
}

fn epic_name_field(spec: &FieldSpec) -> bool {
    if spec.name.to_lowercase() == "epic name" {
        return true;
    }
    spec.custom.contains("gh-epic-label")
}

/// A plain-text paragraph as an Atlassian Document Format document.
fn adf_doc(text: &str) -> Value {
    let text = clip_bytes(text.trim(), MAX_DESCRIPTION);
    json!({
        "type": "doc",
        "version": 1,
        "content": [
            {
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": text },
                ],
            },
        ],
    })
}
