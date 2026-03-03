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
/// Uses header-based column offset parsing to handle the actual tsk output format:
///   ID  Name  Type  Status  Duration  Parent  Agent  Branch  Created
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
    let all_lines: Vec<&str> = output.lines().collect();

    // Pass 1: find the header line and record byte offsets of each column
    let mut header_idx: Option<usize> = None;
    let mut col_id: Option<usize> = None;
    let mut col_name: Option<usize> = None;
    let mut col_type: Option<usize> = None;
    let mut col_status: Option<usize> = None;
    let mut col_duration: Option<usize> = None;
    let mut col_branch: Option<usize> = None;
    let mut col_created: Option<usize> = None;

    for (i, line) in all_lines.iter().enumerate() {
        if line.trim_start().starts_with('-') {
            continue;
        }
        let upper = line.to_uppercase();
        if upper.contains("STATUS") && upper.contains("BRANCH") {
            col_id = upper.find("ID");
            col_name = upper.find("NAME");
            col_type = upper.find("TYPE");
            col_status = upper.find("STATUS");
            col_duration = upper.find("DURATION");
            col_branch = upper.find("BRANCH");
            col_created = upper.find("CREATED");
            header_idx = Some(i);
            break;
        }
    }

    // Require at minimum ID, Name, Status, Branch offsets
    let (id_off, name_off, status_off, branch_off) =
        match (col_id, col_name, col_status, col_branch) {
            (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
            _ => return Ok(tasks), // Unrecognized format
        };

    // Name is bounded by Type (if present) or Status
    let name_end = col_type.unwrap_or(status_off);
    // Status is bounded by Duration (if present) or Branch
    let status_end = col_duration.unwrap_or(branch_off);

    // Pass 2: parse data lines using column offsets
    for (i, line) in all_lines.iter().enumerate() {
        if Some(i) == header_idx {
            continue;
        }
        if line.trim().is_empty() || line.trim_start().starts_with('-') {
            continue;
        }

        let id = extract_field(line, id_off, Some(name_off));
        if id.is_empty() || id.eq_ignore_ascii_case("id") {
            continue;
        }

        let name = extract_field(line, name_off, Some(name_end));
        let status = extract_field(line, status_off, Some(status_end));
        let branch = extract_field(line, branch_off, col_created);

        tasks.push(TskTask {
            id: id.to_string(),
            name: name.to_string(),
            status: status.to_string(),
            branch: branch.to_string(),
        });
    }

    Ok(tasks)
}

/// Extract a field from a fixed-width line by byte offsets, with bounds checking.
fn extract_field(line: &str, start: usize, end: Option<usize>) -> &str {
    let len = line.len();
    if start >= len {
        return "";
    }
    let end = end.unwrap_or(len).min(len);
    line[start..end].trim()
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
        let output = "\
ID        Name             Type     Status     Duration  Parent  Agent   Branch                                Created
REPjpZIU  shell            shell    FAILED     1s        -       claude  tsk/shell/shell/REPjpZIU              2026-03-02 13:33
vf8PWyLH  readme           generic  COMPLETE   1m 3s     -       claude  tsk/generic/readme/vf8PWyLH           2026-03-03 16:45
xYZ12345  long-running     generic  RUNNING    21m 5s    -       claude  tsk/generic/long-running/xYZ12345     2026-03-03 17:00\n";
        let tasks = parse_tsk_list(output).unwrap();
        assert_eq!(tasks.len(), 3);

        assert_eq!(tasks[0].id, "REPjpZIU");
        assert_eq!(tasks[0].name, "shell");
        assert_eq!(tasks[0].status, "FAILED");
        assert_eq!(tasks[0].branch, "tsk/shell/shell/REPjpZIU");

        assert_eq!(tasks[1].id, "vf8PWyLH");
        assert_eq!(tasks[1].name, "readme");
        assert_eq!(tasks[1].status, "COMPLETE");
        assert_eq!(tasks[1].branch, "tsk/generic/readme/vf8PWyLH");

        // Multi-word duration ("21m 5s") must not shift Branch
        assert_eq!(tasks[2].id, "xYZ12345");
        assert_eq!(tasks[2].name, "long-running");
        assert_eq!(tasks[2].status, "RUNNING");
        assert_eq!(tasks[2].branch, "tsk/generic/long-running/xYZ12345");
    }

    #[test]
    fn test_parse_tsk_list_no_header() {
        let output = "some unexpected output\nwith no columns\n";
        let result = parse_tsk_list(output).unwrap();
        assert!(result.is_empty());
    }
}
