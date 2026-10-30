use super::*;

const PAGE_SIZE: usize = 100;

pub(super) fn list(params: &Value) -> Result<Value> {
    let cwd = match params.get("cwd") {
        None | Some(Value::Null) => None,
        Some(Value::String(cwd)) if std::path::Path::new(cwd).is_absolute() => Some(cwd.as_str()),
        _ => anyhow::bail!("cwd must be an absolute path"),
    };
    let cursor = match params.get("cursor") {
        None | Some(Value::Null) => None,
        Some(Value::String(cursor)) => Some(cursor.as_str()),
        _ => anyhow::bail!("cursor must be a string"),
    };
    let directory = crate::storage::jcode_dir()?.join("sessions");
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({"sessions": []}));
        }
        Err(error) => return Err(error.into()),
    };
    // A session id is exactly its file stem (verified across the store), and the
    // result order is by session id. So the ordering can be decided from the
    // directory listing alone, and only the files inside the requested page
    // window need to be parsed. Loading every session before filtering made this
    // cost O(all sessions) file reads: 6k+ files took over 100s.
    let mut ids: Vec<String> = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Some(id) = path.file_stem().and_then(|value| value.to_str()) {
            ids.push(id.to_string());
        }
    }
    ids.sort();
    let index = match cursor {
        None => 0,
        Some(cursor) => ids
            .iter()
            .position(|id| id == cursor)
            .map(|position| position + 1)
            .ok_or_else(|| anyhow::anyhow!("invalid or expired cursor"))?,
    };

    let mut sessions = Vec::new();
    let mut next = index;
    while next < ids.len() && sessions.len() < PAGE_SIZE {
        let end = (next + PAGE_SIZE).min(ids.len());
        for id in &ids[next..end] {
            let Ok(session) = crate::session::Session::load_for_remote_startup(id) else {
                continue;
            };
            let Some(directory) = session.working_dir.as_deref() else {
                continue;
            };
            if !std::path::Path::new(directory).is_absolute()
                || cwd.is_some_and(|cwd| cwd != directory)
            {
                continue;
            }
            sessions.push(json!({"sessionId":session.id,"cwd":directory,
                "title":session.display_title_or_name(),"updatedAt":session.updated_at.to_rfc3339()}));
            if sessions.len() == PAGE_SIZE {
                break;
            }
        }
        next = end;
    }

    let mut result = json!({"sessions": sessions});
    if next < ids.len() {
        if let Some(last) = result["sessions"].as_array().and_then(|rows| rows.last()) {
            result["nextCursor"] = last["sessionId"].clone();
        }
    }
    Ok(result)
}

#[cfg(test)]
fn page(mut sessions: Vec<Value>, cursor: Option<&str>) -> Result<Value> {
    sessions.sort_by(|a, b| a["sessionId"].as_str().cmp(&b["sessionId"].as_str()));
    let start = match cursor {
        None => 0,
        Some(cursor) => sessions
            .iter()
            .position(|session| session["sessionId"].as_str() == Some(cursor))
            .map(|position| position + 1)
            .ok_or_else(|| anyhow::anyhow!("invalid or expired cursor"))?,
    };
    let end = (start + PAGE_SIZE).min(sessions.len());
    let mut result = json!({"sessions": &sessions[start..end]});
    if end < sessions.len() {
        result["nextCursor"] = sessions[end - 1]["sessionId"].clone();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_is_deterministic_and_exhaustive() {
        let rows: Vec<_> = (0..205)
            .rev()
            .map(|n| json!({"sessionId":format!("s{n:03}")}))
            .collect();
        let first = page(rows.clone(), None).unwrap();
        assert_eq!(first["sessions"].as_array().unwrap().len(), 100);
        assert_eq!(first["nextCursor"], "s099");
        let second = page(rows.clone(), first["nextCursor"].as_str()).unwrap();
        let third = page(rows, second["nextCursor"].as_str()).unwrap();
        assert_eq!(third["sessions"].as_array().unwrap().len(), 5);
        assert!(third.get("nextCursor").is_none());
        assert!(page(vec![], Some("unknown")).is_err());
    }
    #[test]
    fn rejects_invalid_filter_before_storage_access() {
        assert!(list(&json!({"cwd":"relative"})).is_err());
        assert!(list(&json!({"cursor":7})).is_err());
    }
}
