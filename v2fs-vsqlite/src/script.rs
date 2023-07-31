use anyhow::Result;
use std::fs;

pub fn load_query_wkld(
    workload_path: &str,
) -> Result<Vec<String>> {
    let mut stms = Vec::new();
    let entire_file = fs::read_to_string(workload_path)?;
    let wkld_sqls = entire_file.split(';');
    for s in wkld_sqls {
        if s.len() <= 1 {
            break;
        }
        if &s[..1] == "\n" {
            stms.push((s[1..]).to_string());
        } else {
            stms.push(s.to_string());
        }
    }
    Ok(stms)
}
