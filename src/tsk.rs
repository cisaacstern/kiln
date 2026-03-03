use anyhow::{Context, Result, bail};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct TskTask {
    pub id: String,
    pub name: String,
    pub status: String,
    pub branch: String,
}

/// Parse `tsk list` output into structured tasks.
/// Expected format from tsk list (tab or whitespace separated):
///   ID  NAME  STATUS  BRANCH
pub fn list_tasks() -> Result<Vec<TskTask>> {
    let output = Command::new("tsk")
        .arg("list")
        .output()
        .context("Failed to run 'tsk list'. Is tsk installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("tsk list failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)?;
    parse_tsk_list(&stdout)
}

fn parse_tsk_list(output: &str) -> Result<Vec<TskTask>> {
    let mut tasks = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip header lines (contain "ID" or start with dashes)
        if line.starts_with('-') || line.to_uppercase().starts_with("ID") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            tasks.push(TskTask {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                status: parts[2].to_string(),
                branch: parts.get(3).unwrap_or(&"").to_string(),
            });
        }
    }

    Ok(tasks)
}

/// Add a new task via `tsk add`, returning (task_id, branch_name)
pub fn add_task(name: &str, prompt_file: &std::path::Path) -> Result<(String, String)> {
    let output = Command::new("tsk")
        .args(["add", "--name", name, "--prompt-file"])
        .arg(prompt_file)
        .output()
        .context("Failed to run 'tsk add'")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("tsk add failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)?;
    parse_tsk_add_output(&stdout, name)
}

fn parse_tsk_add_output(output: &str, name: &str) -> Result<(String, String)> {
    // tsk add typically outputs the task ID and branch info
    // Try to extract ID from output
    let mut id = String::new();
    let mut branch = String::new();

    // Require IDs to contain at least one letter and one digit
    let id_re = regex::Regex::new(r"^[a-zA-Z0-9]{6,16}$").unwrap();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Look for task ID (typically first meaningful output)
        if id.is_empty() {
            for word in line.split_whitespace() {
                let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
                if id_re.is_match(clean)
                    && clean.chars().any(|c| c.is_ascii_alphabetic())
                    && clean.chars().any(|c| c.is_ascii_digit())
                {
                    id = clean.to_string();
                    break;
                }
            }
        }
        // Look for branch name
        if line.contains("tsk/") || line.contains("branch") {
            for word in line.split_whitespace() {
                if word.contains("tsk/") {
                    branch = word.to_string();
                    break;
                }
            }
        }
    }

    if id.is_empty() {
        bail!("Could not parse a valid task ID from tsk add output");
    }

    if branch.is_empty() {
        // Construct expected branch name
        branch = format!("tsk/generic/{}/{}", name, id);
    }

    // Validate the constructed branch name matches git ref pattern
    let ref_re = regex::Regex::new(r"^[a-zA-Z0-9/_.\-]+$").unwrap();
    if !ref_re.is_match(&branch) {
        bail!("Constructed branch name '{}' contains invalid characters", branch);
    }

    Ok((id, branch))
}

/// Find a tsk task by name in the list output
pub fn find_task_by_name(name: &str) -> Result<Option<TskTask>> {
    let tasks = list_tasks()?;
    Ok(tasks.into_iter().find(|t| t.name == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_list() {
        let result = parse_tsk_list("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_tsk_list_with_header() {
        let output = "ID       NAME        STATUS     BRANCH\n\
                       -------- ----------- ---------- --------------------------\n\
                       abc12345 feature-auth COMPLETE   tsk/generic/feature-auth/abc12345\n\
                       def67890 fix-bug      RUNNING    tsk/generic/fix-bug/def67890\n";
        let tasks = parse_tsk_list(output).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "abc12345");
        assert_eq!(tasks[0].name, "feature-auth");
        assert_eq!(tasks[0].status, "COMPLETE");
        assert_eq!(tasks[1].status, "RUNNING");
    }
}
